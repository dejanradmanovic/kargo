//! Operation: update direct dependencies to their latest compatible versions.

use std::path::Path;
use std::sync::Arc;

use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use kargo_core::manifest::Manifest;
use kargo_maven::download;
use kargo_maven::metadata;
use kargo_resolver::resolver;
use kargo_resolver::version::MavenVersion;
use toml_edit::{DocumentMut, Item, Value};

/// Options for `kargo update`.
#[derive(Default)]
pub struct UpdateOptions {
    /// Allow major version bumps.
    pub major: bool,
    /// Only update a specific dependency (artifact name or group:artifact).
    pub dep: Option<String>,
    /// Dry-run: show what would be updated without changing files.
    pub dry_run: bool,
}

struct UpdateEntry {
    key: String,
    group: String,
    artifact: String,
    old_version: String,
    new_version: String,
    section: String,
}

/// Update dependencies in `Kargo.toml` to their latest versions, then re-resolve.
pub async fn update(project_root: &Path, opts: &UpdateOptions) -> miette::Result<()> {
    let manifest_path = project_root.join("Kargo.toml");
    let manifest = Manifest::from_path(&manifest_path)?;
    let repos = resolver::build_repos(&manifest);
    let sp = kargo_util::progress::spinner("Checking for updates...");
    let client = download::build_client()?;

    let mut declared = collect_updatable_deps(&manifest);

    // Include the Kotlin version from [package]
    declared.push((
        "kotlin".to_string(),
        "org.jetbrains.kotlin".to_string(),
        "kotlin-stdlib".to_string(),
        manifest.package.kotlin.clone(),
        "package.kotlin".to_string(),
    ));

    let semaphore = Arc::new(Semaphore::new(8));
    let mut join_set = JoinSet::new();

    for (toml_key, group, artifact, current_version, section) in declared {
        if let Some(ref filter) = opts.dep {
            let matches = filter == &artifact
                || filter == &toml_key
                || filter == "kotlin"
                || *filter == format!("{group}:{artifact}");
            if !matches {
                continue;
            }
        }

        let repos = repos.clone();
        let client = client.clone();
        let sem = semaphore.clone();
        let allow_major = opts.major;

        join_set.spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            for repo in &repos {
                let url = repo.metadata_url(&group, &artifact);
                match download::download_text(&client, repo, &url).await {
                    Ok(Some(xml)) => {
                        if let Ok(meta) = metadata::parse_metadata(&xml) {
                            let best = find_best_update(
                                &current_version,
                                &meta.release.or(meta.latest),
                                &meta.versions,
                                allow_major,
                            );
                            if let Some(new_version) = best {
                                return Ok(Some(UpdateEntry {
                                    key: toml_key,
                                    group,
                                    artifact,
                                    old_version: current_version,
                                    new_version,
                                    section,
                                }));
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

    let mut updates: Vec<UpdateEntry> = Vec::new();
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(Some(entry))) => updates.push(entry),
            Ok(Err(e)) => return Err(e),
            Ok(Ok(None)) => {}
            Err(e) => return Err(miette::miette!("Background task failed: {}", e)),
        }
    }

    sp.finish_and_clear();

    if updates.is_empty() {
        kargo_util::progress::status("Updated", "all dependencies at latest compatible version");
        return Ok(());
    }

    for u in &updates {
        let arrow = if opts.dry_run {
            "would update"
        } else {
            "updated"
        };
        let label = if u.section == "package.kotlin" {
            "kotlin".to_string()
        } else {
            format!("{}:{}", u.group, u.artifact)
        };
        eprintln!(
            "  {} {} {} -> {} [{}]",
            arrow, label, u.old_version, u.new_version, u.section
        );
    }

    if opts.dry_run {
        return Ok(());
    }

    let content = std::fs::read_to_string(&manifest_path).map_err(|e| {
        kargo_util::errors::KargoError::Manifest {
            message: format!("Failed to read Kargo.toml: {e}"),
        }
    })?;

    let mut doc: DocumentMut =
        content
            .parse()
            .map_err(|e| kargo_util::errors::KargoError::Manifest {
                message: format!("Failed to parse Kargo.toml: {e}"),
            })?;

    for u in &updates {
        match u.section.as_str() {
            "package.kotlin" => {
                doc["package"]["kotlin"] = Item::Value(Value::from(u.new_version.clone()));
            }
            "dependencies" => {
                let new_coord = format!("{}:{}:{}", u.group, u.artifact, u.new_version);
                doc["dependencies"][&u.key] = Item::Value(Value::from(new_coord));
            }
            "dev-dependencies" => {
                let new_coord = format!("{}:{}:{}", u.group, u.artifact, u.new_version);
                doc["dev-dependencies"][&u.key] = Item::Value(Value::from(new_coord));
            }
            s if s.starts_with("target.") => {
                let new_coord = format!("{}:{}:{}", u.group, u.artifact, u.new_version);
                let target = &s["target.".len()..];
                doc["target"][target]["dependencies"][&u.key] = Item::Value(Value::from(new_coord));
            }
            _ => {}
        }
    }

    std::fs::write(&manifest_path, doc.to_string()).map_err(kargo_util::errors::KargoError::Io)?;

    eprintln!("Re-resolving dependencies...");
    crate::ops_fetch::fetch(project_root, false).await?;

    eprintln!("Updated {} dependencies.", updates.len());
    Ok(())
}

/// Select the best version to update to.
///
/// Without `--major`, stays within the same major version.
/// Prefers the release/latest marker, falls back to the highest from the versions list.
fn find_best_update(
    current: &str,
    release: &Option<String>,
    versions: &[String],
    allow_major: bool,
) -> Option<String> {
    let current_v = MavenVersion::parse(current);
    let current_major: Option<u64> = current.split('.').next().and_then(|s| s.parse().ok());

    // Filter to candidates that are greater than current
    let mut candidates: Vec<(&str, MavenVersion)> = versions
        .iter()
        .filter_map(|v| {
            if is_prerelease(v) {
                return None;
            }
            let mv = MavenVersion::parse(v);
            if mv > current_v {
                Some((v.as_str(), mv))
            } else {
                None
            }
        })
        .collect();

    if !allow_major {
        if let Some(cm) = current_major {
            candidates.retain(|(v, _)| {
                v.split('.')
                    .next()
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(|m| m == cm)
                    .unwrap_or(false)
            });
        }
    }

    // Also consider release/latest marker if it passes filters
    if let Some(ref r) = release {
        let rv = MavenVersion::parse(r);
        if rv > current_v {
            let passes_major = allow_major
                || current_major.map_or(true, |cm| {
                    r.split('.')
                        .next()
                        .and_then(|s| s.parse::<u64>().ok())
                        .map(|m| m == cm)
                        .unwrap_or(false)
                });
            let not_prerelease = !is_prerelease(r);
            if passes_major && not_prerelease {
                candidates.push((r.as_str(), rv));
            }
        }
    }

    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    candidates.first().map(|(v, _)| v.to_string())
}

/// Collect updatable direct dependencies: `(toml_key, group, artifact, version, section)`.
fn collect_updatable_deps(manifest: &Manifest) -> Vec<(String, String, String, String, String)> {
    use kargo_core::dependency::{Dependency, MavenCoordinate};

    let mut deps = Vec::new();

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

    for (key, dep) in &manifest.dependencies {
        if let Some((g, a, v)) = extract(dep) {
            deps.push((key.clone(), g, a, v, "dependencies".to_string()));
        }
    }
    for (key, dep) in &manifest.dev_dependencies {
        if let Some((g, a, v)) = extract(dep) {
            deps.push((key.clone(), g, a, v, "dev-dependencies".to_string()));
        }
    }
    for (target_name, target_deps) in &manifest.target {
        for (key, dep) in &target_deps.dependencies {
            if let Some((g, a, v)) = extract(dep) {
                deps.push((key.clone(), g, a, v, format!("target.{target_name}")));
            }
        }
    }

    deps
}

fn is_prerelease(version: &str) -> bool {
    let lower = version.to_lowercase();
    lower.contains("-snapshot")
        || lower.contains("-alpha")
        || lower.contains("-beta")
        || lower.contains("-rc")
        || lower.contains("-dev")
        || lower.contains("-eap")
        || lower.contains("-m")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_update_same_major() {
        let versions = vec![
            "1.6.0".to_string(),
            "1.7.0".to_string(),
            "1.8.0".to_string(),
            "2.0.0".to_string(),
        ];
        let best = find_best_update("1.7.0", &Some("2.0.0".to_string()), &versions, false);
        assert_eq!(best, Some("1.8.0".to_string()));
    }

    #[test]
    fn find_update_allow_major() {
        let versions = vec![
            "1.8.0".to_string(),
            "2.0.0".to_string(),
            "2.1.0".to_string(),
        ];
        let best = find_best_update("1.7.0", &Some("2.1.0".to_string()), &versions, true);
        assert_eq!(best, Some("2.1.0".to_string()));
    }

    #[test]
    fn no_update_available() {
        let versions = vec!["1.0.0".to_string(), "1.1.0".to_string()];
        let best = find_best_update("1.1.0", &Some("1.1.0".to_string()), &versions, false);
        assert_eq!(best, None);
    }

    #[test]
    fn skips_prerelease() {
        let versions = vec![
            "1.0.0".to_string(),
            "1.1.0-beta".to_string(),
            "1.1.0-RC1".to_string(),
            "1.2.0-SNAPSHOT".to_string(),
        ];
        let best = find_best_update("1.0.0", &None, &versions, false);
        assert_eq!(best, None);
    }
}
