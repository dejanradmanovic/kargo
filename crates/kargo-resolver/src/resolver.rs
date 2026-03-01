//! Core dependency resolution algorithm: nearest-wins BFS, scope propagation,
//! exclusions, optional dependency handling, and BOM imports.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use kargo_core::dependency::{Dependency, MavenCoordinate};
use kargo_core::lockfile::Lockfile;
use kargo_core::manifest::Manifest;
use kargo_maven::cache::LocalCache;
use kargo_maven::pom::Pom;
use kargo_maven::repository::MavenRepository;
use reqwest::Client;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::conflict::{ConflictReport, VersionConflict};
use crate::graph::{DepEdge, DependencyGraph, ResolvedNode};

const MAX_CONCURRENT_FETCHES: usize = 8;

/// The output of dependency resolution.
pub struct ResolutionResult {
    pub graph: DependencyGraph,
    pub conflicts: ConflictReport,
    /// All resolved artifacts as flat coordinates for lockfile generation.
    pub artifacts: Vec<ResolvedArtifact>,
    /// All versions requested for each `group:artifact` during resolution.
    /// Artifacts with more than one entry were requested at multiple versions.
    pub version_requests: HashMap<String, HashSet<String>>,
}

/// A single resolved artifact with its source repository.
#[derive(Debug, Clone)]
pub struct ResolvedArtifact {
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub scope: String,
    pub source: String,
    pub checksum: Option<String>,
    pub dependencies: Vec<ArtifactRef>,
}

/// A reference to a dependency within a resolved artifact.
#[derive(Debug, Clone)]
pub struct ArtifactRef {
    pub group: String,
    pub artifact: String,
    pub version: String,
}

/// Entry in the BFS queue.
struct QueueEntry {
    group: String,
    artifact: String,
    version: String,
    scope: String,
    depth: usize,
    parent_key: Option<String>,
    exclusions: HashSet<String>,
}

/// Resolve all dependencies declared in a manifest.
///
/// Uses BFS with Maven's "nearest wins" strategy.
pub async fn resolve(
    manifest: &Manifest,
    repos: &[MavenRepository],
    cache: &LocalCache,
    lockfile: Option<&Lockfile>,
    client: &Client,
) -> miette::Result<ResolutionResult> {
    let mut graph = DependencyGraph::new();
    let mut conflicts = ConflictReport::new();

    let root = graph.add_node(ResolvedNode {
        group: manifest.package.group.clone().unwrap_or_default(),
        artifact: manifest.package.name.clone(),
        version: manifest.package.version.clone(),
        scope: "compile".to_string(),
    });
    graph.set_root(root);

    // Collect direct deps from all sections
    let mut direct_deps = Vec::new();
    for (name, dep) in &manifest.dependencies {
        if let Some(coord) = resolve_dep_coordinate(dep, name, manifest) {
            direct_deps.push((coord, "compile".to_string()));
        }
    }
    for (name, dep) in &manifest.dev_dependencies {
        if let Some(coord) = resolve_dep_coordinate(dep, name, manifest) {
            direct_deps.push((coord, "test".to_string()));
        }
    }
    // Per-target deps
    for target_deps in manifest.target.values() {
        for (name, dep) in &target_deps.dependencies {
            if let Some(coord) = resolve_dep_coordinate(dep, name, manifest) {
                direct_deps.push((coord, "compile".to_string()));
            }
        }
    }
    // KSP processor deps — build-time only, excluded from runtime classpath
    for (name, dep) in &manifest.ksp {
        if let Some(coord) = resolve_dep_coordinate(dep, name, manifest) {
            direct_deps.push((coord, "ksp".to_string()));
        }
    }
    // KAPT processor deps — build-time only, excluded from runtime classpath
    for (name, dep) in &manifest.kapt {
        if let Some(coord) = resolve_dep_coordinate(dep, name, manifest) {
            direct_deps.push((coord, "kapt".to_string()));
        }
    }

    // Build lock index and determine which subtrees are stale.
    // Direct deps always use the manifest version. If a direct dep's version
    // differs from the lockfile, its entire transitive subtree is re-resolved.
    let full_lock_index = build_lock_index(lockfile);
    let stale_keys = compute_stale_keys(&direct_deps, lockfile);
    let locked_versions: HashMap<String, String> = full_lock_index
        .into_iter()
        .filter(|(k, _)| !stale_keys.contains(k))
        .collect();

    let mut queue: VecDeque<QueueEntry> = VecDeque::new();
    let mut resolved: HashMap<String, (String, usize)> = HashMap::new();
    let mut version_requests: HashMap<String, HashSet<String>> = HashMap::new();
    let mut pom_cache: HashMap<String, Pom> = HashMap::new();

    let direct_keys: HashSet<String> = direct_deps
        .iter()
        .map(|(c, _)| format!("{}:{}", c.group_id, c.artifact_id))
        .collect();

    for (coord, scope) in &direct_deps {
        queue.push_back(QueueEntry {
            group: coord.group_id.clone(),
            artifact: coord.artifact_id.clone(),
            version: coord.version.clone(),
            scope: scope.clone(),
            depth: 1,
            parent_key: None,
            exclusions: HashSet::new(),
        });
    }

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_FETCHES));

    while !queue.is_empty() {
        // Drain the current depth level from the front of the queue
        let current_depth = queue.front().map(|e| e.depth).unwrap_or(0);
        let mut level: Vec<QueueEntry> = Vec::new();
        while queue.front().is_some_and(|e| e.depth == current_depth) {
            level.push(queue.pop_front().unwrap());
        }

        // Prefetch POMs for this level in parallel
        let coords_to_fetch: Vec<(String, String, String)> = level
            .iter()
            .map(|e| (e.group.clone(), e.artifact.clone(), e.version.clone()))
            .filter(|(g, a, v)| {
                let k = format!("{g}:{a}:{v}");
                !pom_cache.contains_key(&k)
            })
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        if !coords_to_fetch.is_empty() {
            let mut join_set = JoinSet::new();
            for (group, artifact, version) in coords_to_fetch {
                let client = client.clone();
                let repos = repos.to_vec();
                let cache_root = cache.root().to_path_buf();
                let sem = semaphore.clone();
                join_set.spawn(async move {
                    let _permit = sem.acquire().await;
                    let local_cache = LocalCache::from_root(cache_root);
                    let result =
                        fetch_pom_from_repos(&client, &repos, &local_cache, &group, &artifact, &version)
                            .await;
                    (format!("{group}:{artifact}:{version}"), result)
                });
            }
            while let Some(result) = join_set.join_next().await {
                if let Ok((coord_key, Ok(Some(pom)))) = result {
                    pom_cache.insert(coord_key, pom);
                }
            }
        }

        // Process entries at this depth level
        for entry in level {
            let key = format!("{}:{}", entry.group, entry.artifact);

            version_requests
                .entry(key.clone())
                .or_default()
                .insert(entry.version.clone());

            if let Some((existing_ver, existing_depth)) = resolved.get(&key) {
                if *existing_depth <= entry.depth {
                    if *existing_ver != entry.version {
                        conflicts.add(VersionConflict {
                            group: entry.group.clone(),
                            artifact: entry.artifact.clone(),
                            requested: entry.version.clone(),
                            resolved: existing_ver.clone(),
                            reason: format!(
                                "nearest wins (depth {} vs {})",
                                existing_depth, entry.depth
                            ),
                        });
                    }
                    continue;
                }
            }

            resolved.insert(key.clone(), (entry.version.clone(), entry.depth));

            let node = graph.add_node(ResolvedNode {
                group: entry.group.clone(),
                artifact: entry.artifact.clone(),
                version: entry.version.clone(),
                scope: entry.scope.clone(),
            });

            if let Some(ref parent_key) = entry.parent_key {
                if let Some(parent_idx) = graph.find(parent_key) {
                    graph.add_edge(
                        parent_idx,
                        node,
                        DepEdge {
                            scope: entry.scope.clone(),
                            optional: false,
                        },
                    );
                }
            } else {
                graph.add_edge(
                    root,
                    node,
                    DepEdge {
                        scope: entry.scope.clone(),
                        optional: false,
                    },
                );
            }

            let coord_key = format!("{}:{}:{}", entry.group, entry.artifact, entry.version);
            let pom = pom_cache.get(&coord_key).cloned();

            if let Some(mut pom) = pom {
                pom.resolve_properties();

                for dep in &pom.dependencies {
                    if dep.optional {
                        continue;
                    }
                    let dep_scope = dep.scope.as_deref().unwrap_or("compile");
                    if dep_scope == "test" || dep_scope == "provided" || dep_scope == "system" {
                        continue;
                    }

                    let dep_key = format!("{}:{}", dep.group_id, dep.artifact_id);

                    if entry.exclusions.contains(&dep_key)
                        || entry.exclusions.contains(&dep.group_id)
                    {
                        continue;
                    }

                    let version = dep
                        .version
                        .clone()
                        .or_else(|| {
                            pom.managed_version(&dep.group_id, &dep.artifact_id)
                                .map(|s| s.to_string())
                        })
                        .unwrap_or_default();

                    if version.is_empty() {
                        continue;
                    }

                    let dep_key = format!("{}:{}", dep.group_id, dep.artifact_id);
                    let version = if !direct_keys.contains(&dep_key) {
                        locked_versions.get(&dep_key).cloned().unwrap_or(version)
                    } else {
                        version
                    };

                    let propagated_scope = propagate_scope(&entry.scope, dep_scope);

                    let mut child_exclusions = entry.exclusions.clone();
                    for excl in &dep.exclusions {
                        if let Some(ref art) = excl.artifact_id {
                            child_exclusions.insert(format!("{}:{}", excl.group_id, art));
                        } else {
                            child_exclusions.insert(excl.group_id.clone());
                        }
                    }

                    queue.push_back(QueueEntry {
                        group: dep.group_id.clone(),
                        artifact: dep.artifact_id.clone(),
                        version,
                        scope: propagated_scope,
                        depth: entry.depth + 1,
                        parent_key: Some(key.clone()),
                        exclusions: child_exclusions,
                    });
                }
            }
        }
    }

    // Build flat artifact list for lockfile
    let artifacts = build_artifact_list(&graph, &pom_cache, repos);

    Ok(ResolutionResult {
        graph,
        conflicts,
        artifacts,
        version_requests,
    })
}

/// Resolve a `Dependency` enum to `MavenCoordinate`.
fn resolve_dep_coordinate(
    dep: &Dependency,
    _name: &str,
    manifest: &Manifest,
) -> Option<MavenCoordinate> {
    match dep {
        Dependency::Short(s) => MavenCoordinate::parse(s),
        Dependency::Detailed(d) => Some(MavenCoordinate {
            group_id: d.group.clone(),
            artifact_id: d.artifact.clone(),
            version: d.version.clone(),
        }),
        Dependency::Catalog(c) => {
            let catalog = manifest.catalog.as_ref()?;
            let lib = catalog.libraries.get(&c.catalog)?;
            let version = if let Some(ref vref) = lib.version_ref {
                catalog.versions.get(vref).cloned().unwrap_or_default()
            } else {
                lib.version.clone().unwrap_or_default()
            };
            Some(MavenCoordinate {
                group_id: lib.group.clone(),
                artifact_id: lib.artifact.clone(),
                version,
            })
        }
    }
}

/// Build a lookup from `group:artifact` to locked version.
fn build_lock_index(lockfile: Option<&Lockfile>) -> HashMap<String, String> {
    let mut index = HashMap::new();
    if let Some(lf) = lockfile {
        for pkg in &lf.package {
            index.insert(format!("{}:{}", pkg.group, pkg.name), pkg.version.clone());
        }
    }
    index
}

/// Identify all lockfile entries that are stale because a direct dependency changed.
///
/// Walks the lockfile's dependency graph starting from changed direct deps
/// to find their entire transitive subtree. These entries must be re-resolved
/// from POMs rather than pinned from the lockfile.
fn compute_stale_keys(
    direct_deps: &[(MavenCoordinate, String)],
    lockfile: Option<&Lockfile>,
) -> HashSet<String> {
    let mut stale = HashSet::new();
    let lf = match lockfile {
        Some(lf) => lf,
        None => return stale,
    };

    // Build adjacency list from lockfile
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for pkg in &lf.package {
        let key = format!("{}:{}", pkg.group, pkg.name);
        let deps: Vec<String> = pkg
            .dependencies
            .iter()
            .map(|d| format!("{}:{}", d.group, d.name))
            .collect();
        children.insert(key, deps);
    }

    // Find direct deps whose version changed vs lockfile
    let mut roots: Vec<String> = Vec::new();
    for (coord, _) in direct_deps {
        let key = format!("{}:{}", coord.group_id, coord.artifact_id);
        let locked_ver = lf.locked_version(&coord.group_id, &coord.artifact_id);
        match locked_ver {
            Some(v) if v == coord.version => {} // unchanged
            _ => roots.push(key),               // changed or new
        }
    }

    // BFS from changed roots to mark their subtrees stale
    let mut visit_queue: VecDeque<String> = roots.into_iter().collect();
    while let Some(key) = visit_queue.pop_front() {
        if !stale.insert(key.clone()) {
            continue;
        }
        if let Some(deps) = children.get(&key) {
            for dep in deps {
                if !stale.contains(dep) {
                    visit_queue.push_back(dep.clone());
                }
            }
        }
    }

    stale
}

/// Fetch a POM from the first repository that has it.
async fn fetch_pom_from_repos(
    client: &Client,
    repos: &[MavenRepository],
    cache: &LocalCache,
    group: &str,
    artifact: &str,
    version: &str,
) -> miette::Result<Option<Pom>> {
    // Check cache first
    if let Some(pom) = cache.get_pom(group, artifact, version) {
        return Ok(Some(pom));
    }

    for repo in repos {
        match cache
            .fetch_pom(client, repo, group, artifact, version)
            .await?
        {
            Some(pom) => return Ok(Some(pom)),
            None => continue,
        }
    }

    Ok(None)
}

/// Maven scope propagation rules.
/// Processor scopes (`ksp`, `kapt`) propagate like `test`: all transitive
/// deps inherit the processor scope so they stay out of the runtime classpath.
fn propagate_scope(parent_scope: &str, dep_scope: &str) -> String {
    match (parent_scope, dep_scope) {
        ("compile", "compile") => "compile",
        ("compile", "runtime") => "runtime",
        ("runtime", "compile") => "runtime",
        ("runtime", "runtime") => "runtime",
        ("test", _) => "test",
        (_, "test") => "test",
        ("ksp", _) => "ksp",
        ("kapt", _) => "kapt",
        (_, "provided") => "provided",
        _ => "compile",
    }
    .to_string()
}

/// Build a flat list of resolved artifacts from the graph.
fn build_artifact_list(
    graph: &DependencyGraph,
    _pom_cache: &HashMap<String, Pom>,
    repos: &[MavenRepository],
) -> Vec<ResolvedArtifact> {
    let mut artifacts = Vec::new();
    for node in graph.all_nodes() {
        let source = repos.first().map(|r| r.url.clone()).unwrap_or_default();

        let node_idx = match graph.find(&node.key()) {
            Some(idx) => idx,
            None => continue,
        };
        let deps: Vec<ArtifactRef> = graph
            .dependencies_of(node_idx)
            .iter()
            .map(|(idx, _)| {
                let child = graph.node(*idx);
                ArtifactRef {
                    group: child.group.clone(),
                    artifact: child.artifact.clone(),
                    version: child.version.clone(),
                }
            })
            .collect();

        artifacts.push(ResolvedArtifact {
            group: node.group.clone(),
            artifact: node.artifact.clone(),
            version: node.version.clone(),
            scope: node.scope.clone(),
            source,
            checksum: None,
            dependencies: deps,
        });
    }

    artifacts.sort_by(|a, b| (&a.group, &a.artifact).cmp(&(&b.group, &b.artifact)));
    artifacts
}

/// Build the list of repositories from a manifest, always including Maven Central.
pub fn build_repos(manifest: &Manifest) -> Vec<MavenRepository> {
    let mut repos = Vec::new();
    for (name, entry) in &manifest.repositories {
        repos.push(MavenRepository::from_entry(name, entry));
    }
    if repos.is_empty()
        || !repos
            .iter()
            .any(|r| r.url.contains("repo.maven.apache.org"))
    {
        repos.push(MavenRepository::maven_central());
    }
    repos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_propagation() {
        assert_eq!(propagate_scope("compile", "compile"), "compile");
        assert_eq!(propagate_scope("compile", "runtime"), "runtime");
        assert_eq!(propagate_scope("runtime", "compile"), "runtime");
        assert_eq!(propagate_scope("test", "compile"), "test");
        assert_eq!(propagate_scope("compile", "provided"), "provided");
    }

    #[test]
    fn resolve_short_dep() {
        let dep =
            Dependency::Short("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0".to_string());
        let manifest = Manifest::parse_toml(
            r#"
[package]
name = "test"
version = "0.1.0"
kotlin = "2.3.0"
"#,
        )
        .unwrap();
        let coord = resolve_dep_coordinate(&dep, "coroutines", &manifest).unwrap();
        assert_eq!(coord.group_id, "org.jetbrains.kotlinx");
        assert_eq!(coord.artifact_id, "kotlinx-coroutines-core");
        assert_eq!(coord.version, "1.8.0");
    }

    #[test]
    fn lock_index_lookup() {
        let lockfile = Lockfile {
            package: vec![kargo_core::lockfile::LockedPackage {
                name: "kotlinx-coroutines-core".to_string(),
                group: "org.jetbrains.kotlinx".to_string(),
                version: "1.8.0".to_string(),
                checksum: None,
                source: None,
                scope: None,
                targets: vec![],
                dependencies: vec![],
            }],
        };
        let idx = build_lock_index(Some(&lockfile));
        assert_eq!(
            idx.get("org.jetbrains.kotlinx:kotlinx-coroutines-core"),
            Some(&"1.8.0".to_string())
        );
    }

    #[test]
    fn build_repos_includes_central() {
        let manifest = Manifest::parse_toml(
            r#"
[package]
name = "test"
version = "0.1.0"
kotlin = "2.3.0"
"#,
        )
        .unwrap();
        let repos = build_repos(&manifest);
        assert!(!repos.is_empty());
        assert!(repos.iter().any(|r| r.url.contains("maven.apache.org")));
    }
}
