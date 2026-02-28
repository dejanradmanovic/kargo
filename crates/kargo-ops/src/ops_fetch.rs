//! Operation: resolve and download all dependencies.

use std::collections::HashMap;
use std::path::Path;

use kargo_core::lockfile::{Lockfile, ResolvedPackageInfo};
use kargo_core::manifest::Manifest;
use kargo_maven::cache::LocalCache;
use kargo_maven::download;
use kargo_resolver::resolver::{self, ResolutionResult};
use kargo_util::hash::sha256_bytes;

/// Fetch all dependencies: resolve, download artifacts to the project cache,
/// and update the lockfile.
pub async fn fetch(project_root: &Path, verbose: bool) -> miette::Result<()> {
    let manifest_path = project_root.join("Kargo.toml");
    let manifest = Manifest::from_path(&manifest_path)?;
    let repos = resolver::build_repos(&manifest);
    let cache = LocalCache::new(project_root);

    let lockfile_path = project_root.join("Kargo.lock");
    let existing_lock = if lockfile_path.is_file() {
        Lockfile::from_path(&lockfile_path).ok()
    } else {
        None
    };

    let client = download::build_client()?;
    let result =
        resolver::resolve(&manifest, &repos, &cache, existing_lock.as_ref(), &client).await?;

    if !result.conflicts.is_empty() && verbose {
        eprintln!("{}", result.conflicts);
    }

    let artifact_count = result.artifacts.len();
    let mut downloaded = 0u32;
    let mut up_to_date = 0u32;
    let mut checksums: HashMap<String, String> = HashMap::new();

    for artifact in &result.artifacts {
        let coord_key = format!(
            "{}:{}:{}",
            artifact.group, artifact.artifact, artifact.version
        );

        if let Some(jar_path) =
            cache.get_jar(&artifact.group, &artifact.artifact, &artifact.version, None)
        {
            up_to_date += 1;
            if let Ok(data) = std::fs::read(&jar_path) {
                checksums.insert(coord_key, sha256_bytes(&data));
            }
            continue;
        }

        let mut found = false;
        for repo in &repos {
            let url = repo.jar_url(&artifact.group, &artifact.artifact, &artifact.version, None);
            let label = format!("{}:{}", artifact.artifact, artifact.version);
            match download::download_artifact(&client, repo, &url, &label).await? {
                Some(data) => {
                    kargo_maven::checksum::verify(&client, repo, &url, &data).await?;
                    checksums.insert(coord_key.clone(), sha256_bytes(&data));
                    cache.put_jar(
                        &artifact.group,
                        &artifact.artifact,
                        &artifact.version,
                        None,
                        &data,
                    )?;
                    downloaded += 1;
                    found = true;
                    break;
                }
                None => continue,
            }
        }

        if !found && verbose {
            eprintln!(
                "  Warning: JAR not found for {}:{}:{}",
                artifact.group, artifact.artifact, artifact.version
            );
        }
    }

    // Prune stale artifacts no longer in the resolved set
    let keep: std::collections::HashSet<(String, String, String)> = result
        .artifacts
        .iter()
        .map(|a| (a.group.clone(), a.artifact.clone(), a.version.clone()))
        .collect();
    let pruned = cache.prune(&keep);

    let lock_packages = resolution_to_lockfile_packages(&result, &checksums);
    let lockfile = Lockfile::generate(lock_packages);
    lockfile.write_to(&lockfile_path)?;

    if downloaded > 0 || pruned > 0 || verbose {
        eprintln!(
            "Resolved {artifact_count} dependencies, downloaded {downloaded}, \
             {up_to_date} up-to-date, {pruned} pruned"
        );
    } else if artifact_count > 0 {
        eprintln!("All {artifact_count} dependencies up-to-date");
    }

    Ok(())
}

/// Collect `(group, artifact, version)` from all direct dependency sections.
pub fn collect_declared_deps(manifest: &Manifest) -> Vec<(String, String, String)> {
    use kargo_core::dependency::{Dependency, MavenCoordinate};

    let mut declared = Vec::new();

    let extract = |dep: &Dependency| -> Option<(String, String, String)> {
        match dep {
            Dependency::Short(s) => {
                let coord = MavenCoordinate::parse(s)?;
                Some((coord.group_id, coord.artifact_id, coord.version))
            }
            Dependency::Detailed(d) => {
                Some((d.group.clone(), d.artifact.clone(), d.version.clone()))
            }
            Dependency::Catalog(_) => None,
        }
    };

    for dep in manifest.dependencies.values() {
        if let Some(t) = extract(dep) {
            declared.push(t);
        }
    }
    for dep in manifest.dev_dependencies.values() {
        if let Some(t) = extract(dep) {
            declared.push(t);
        }
    }
    for target_deps in manifest.target.values() {
        for dep in target_deps.dependencies.values() {
            if let Some(t) = extract(dep) {
                declared.push(t);
            }
        }
    }

    declared
}

/// Convert resolution results into lockfile package descriptors.
pub fn resolution_to_lockfile_packages(
    result: &ResolutionResult,
    checksums: &HashMap<String, String>,
) -> Vec<ResolvedPackageInfo> {
    result
        .artifacts
        .iter()
        .map(|a| {
            let coord_key = format!("{}:{}:{}", a.group, a.artifact, a.version);
            ResolvedPackageInfo {
                group: a.group.clone(),
                artifact: a.artifact.clone(),
                version: a.version.clone(),
                scope: Some(a.scope.clone()),
                source: Some(a.source.clone()),
                checksum: checksums.get(&coord_key).cloned(),
                targets: vec![],
                dependencies: a
                    .dependencies
                    .iter()
                    .map(|d| (d.group.clone(), d.artifact.clone(), d.version.clone()))
                    .collect(),
            }
        })
        .collect()
}
