//! Operation: check for outdated direct dependencies.

use std::path::Path;
use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use kargo_core::manifest::Manifest;
use kargo_maven::download;
use kargo_maven::metadata;
use kargo_resolver::resolver;
use kargo_resolver::version::MavenVersion;

/// Options for `kargo outdated`.
#[derive(Default)]
pub struct OutdatedOptions {
    /// Include major version bumps.
    pub major: bool,
}

/// A single outdated dependency entry.
struct OutdatedEntry {
    group: String,
    artifact: String,
    current: String,
    latest: String,
    is_major: bool,
    section: String,
}

/// Check direct dependencies for available updates and print a report.
pub async fn outdated(project_root: &Path, opts: &OutdatedOptions) -> miette::Result<()> {
    let manifest_path = project_root.join("Kargo.toml");
    let manifest = Manifest::from_path(&manifest_path)?;
    let repos = resolver::build_repos(&manifest);
    let sp = kargo_util::progress::spinner("Checking for outdated dependencies...");
    let client = download::build_client()?;

    let mut declared = collect_declared_deps_with_section(&manifest);

    // Include the Kotlin version from [package]
    declared.push((
        "org.jetbrains.kotlin".to_string(),
        "kotlin-stdlib".to_string(),
        manifest.package.kotlin.clone(),
        "package.kotlin".to_string(),
    ));

    let semaphore = Arc::new(Semaphore::new(8));
    let mut join_set = JoinSet::new();

    for (group, artifact, version, section) in declared {
        let repos = repos.clone();
        let client = client.clone();
        let sem = semaphore.clone();

        join_set.spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            for repo in &repos {
                let url = repo.metadata_url(&group, &artifact);
                match download::download_text(&client, repo, &url).await {
                    Ok(Some(xml)) => {
                        if let Ok(meta) = metadata::parse_metadata(&xml) {
                            if let Some(ref latest) = meta.release.or(meta.latest) {
                                let current = MavenVersion::parse(&version);
                                let latest_v = MavenVersion::parse(latest);
                                if latest_v > current {
                                    let is_major = is_major_bump(&version, latest);
                                    return Ok(Some(OutdatedEntry {
                                        group,
                                        artifact,
                                        current: version,
                                        latest: latest.clone(),
                                        is_major,
                                        section,
                                    }));
                                }
                            }
                        }
                        break;
                    }
                    Ok(None) => continue,
                    Err(e) => return Err(e),
                }
            }
            Ok(None)
        });
    }

    let mut entries: Vec<OutdatedEntry> = Vec::new();
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(Some(entry))) => entries.push(entry),
            Ok(Err(e)) => return Err(e),
            Ok(Ok(None)) => {}
            Err(e) => return Err(miette::miette!("Background task failed: {}", e)),
        }
    }

    sp.finish_and_clear();

    if entries.is_empty() {
        kargo_util::progress::status("Outdated", "all dependencies are up to date");
        return Ok(());
    }

    println!(
        "{:<50} {:<15} {:<15} Section",
        "Dependency", "Current", "Latest"
    );
    println!("{}", "-".repeat(90));

    for entry in &entries {
        if !opts.major && entry.is_major {
            continue;
        }
        let name = if entry.section == "package.kotlin" {
            "kotlin".to_string()
        } else {
            format!("{}:{}", entry.group, entry.artifact)
        };
        let marker = if entry.is_major { " (major)" } else { "" };
        println!(
            "{:<50} {:<15} {:<15} {}{}",
            name, entry.current, entry.latest, entry.section, marker
        );
    }

    Ok(())
}

/// Collect direct dependencies with their section label for display.
fn collect_declared_deps_with_section(
    manifest: &Manifest,
) -> Vec<(String, String, String, String)> {
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
        if let Some((g, a, v)) = extract(dep) {
            declared.push((g, a, v, "dependencies".to_string()));
        }
    }
    for dep in manifest.dev_dependencies.values() {
        if let Some((g, a, v)) = extract(dep) {
            declared.push((g, a, v, "dev-dependencies".to_string()));
        }
    }
    for (target_name, target_deps) in &manifest.target {
        for dep in target_deps.dependencies.values() {
            if let Some((g, a, v)) = extract(dep) {
                declared.push((g, a, v, format!("target.{target_name}")));
            }
        }
    }
    for dep in manifest.ksp.values() {
        if let Some((g, a, v)) = extract(dep) {
            declared.push((g, a, v, "ksp".to_string()));
        }
    }
    for dep in manifest.kapt.values() {
        if let Some((g, a, v)) = extract(dep) {
            declared.push((g, a, v, "kapt".to_string()));
        }
    }

    declared
}

/// Quick heuristic: check if the first numeric segment differs.
fn is_major_bump(current: &str, latest: &str) -> bool {
    let c_major = current
        .split('.')
        .next()
        .and_then(|s| s.parse::<u64>().ok());
    let l_major = latest.split('.').next().and_then(|s| s.parse::<u64>().ok());
    match (c_major, l_major) {
        (Some(c), Some(l)) => c != l,
        _ => false,
    }
}
