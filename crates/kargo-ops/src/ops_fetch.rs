//! Operation: resolve and download all dependencies.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use kargo_core::lockfile::{Lockfile, ResolvedPackageInfo};
use kargo_core::manifest::Manifest;
use kargo_maven::cache::LocalCache;
use kargo_maven::download;
use kargo_resolver::resolver::{self, ResolutionResult};
use kargo_util::hash::sha256_bytes;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const MAX_CONCURRENT_DOWNLOADS: usize = 8;

/// Fetch all dependencies: resolve, download artifacts to the project cache,
/// and update the lockfile.
pub async fn fetch(project_root: &Path, verbose: bool) -> miette::Result<()> {
    use kargo_util::progress::{spinner, status};

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

    let sp = spinner("Resolving dependencies...");
    let client = download::build_client()?;
    let result =
        resolver::resolve(&manifest, &repos, &cache, existing_lock.as_ref(), &client).await?;
    sp.finish_and_clear();

    if !result.conflicts.is_empty() && verbose {
        eprintln!("{}", result.conflicts);
    }

    let artifact_count = result.artifacts.len();
    let mut downloaded = 0u32;
    let mut up_to_date = 0u32;
    let mut checksums: HashMap<String, String> = HashMap::new();

    // Separate cached artifacts from those needing download
    let mut to_download = Vec::new();
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
        } else {
            to_download.push((artifact, coord_key));
        }
    }

    let dl_sp = spinner(&format!("Downloading {artifact_count} dependencies..."));
    if !to_download.is_empty() {
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_DOWNLOADS));
        let mut join_set = JoinSet::new();

        for (artifact, coord_key) in &to_download {
            let sem = semaphore.clone();
            let client = client.clone();
            let repos = repos.clone();
            let group = artifact.group.clone();
            let artifact_name = artifact.artifact.clone();
            let version = artifact.version.clone();
            let coord_key = coord_key.clone();
            let cache_root = cache.root().to_path_buf();

            join_set.spawn(async move {
                let _permit = sem.acquire().await;
                let local_cache = LocalCache::from_root(cache_root);
                for repo in &repos {
                    let url = repo.jar_url(&group, &artifact_name, &version, None);
                    let label = format!("{artifact_name}:{version}");
                    match download::download_artifact(&client, repo, &url, &label).await {
                        Ok(Some(data)) => {
                            if let Err(e) =
                                kargo_maven::checksum::verify(&client, repo, &url, &data).await
                            {
                                return Err(e);
                            }
                            let checksum = sha256_bytes(&data);
                            local_cache.put_jar(&group, &artifact_name, &version, None, &data)?;
                            return Ok(Some((coord_key, checksum)));
                        }
                        Ok(None) => continue,
                        Err(e) => return Err(e),
                    }
                }
                Ok(None)
            });
        }

        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Ok(Some((coord_key, checksum)))) => {
                    checksums.insert(coord_key, checksum);
                    downloaded += 1;
                }
                Ok(Ok(None)) => {
                    // Not found in any repo (warning handled below)
                }
                Ok(Err(e)) => return Err(e),
                Err(e) => {
                    return Err(kargo_util::errors::KargoError::Generic {
                        message: format!("Download task failed: {e}"),
                    }
                    .into())
                }
            }
        }

        // Report warnings for artifacts not found
        if verbose {
            let downloaded_keys: std::collections::HashSet<_> = checksums.keys().collect();
            for (artifact, coord_key) in &to_download {
                if !downloaded_keys.contains(coord_key) {
                    kargo_util::progress::status_warn(
                        "Warning",
                        &format!(
                            "JAR not found for {}:{}:{}",
                            artifact.group, artifact.artifact, artifact.version
                        ),
                    );
                }
            }
        }
    }
    dl_sp.finish_and_clear();

    // Prune stale artifacts no longer in the resolved set.
    // Also protect auto-provisioned JARs (KSP toolchain, JUnit runner) that
    // are downloaded outside the normal resolve→fetch cycle.
    let mut keep: std::collections::HashSet<(String, String, String)> = result
        .artifacts
        .iter()
        .map(|a| (a.group.clone(), a.artifact.clone(), a.version.clone()))
        .collect();

    // JUnit platform (auto-provisioned by `kargo test`)
    let has_kotlin_test = manifest.dev_dependencies.values().any(|dep| {
        let coord = match dep {
            kargo_core::dependency::Dependency::Short(s) => s.as_str(),
            kargo_core::dependency::Dependency::Detailed(d) => d.artifact.as_str(),
            kargo_core::dependency::Dependency::Catalog(c) => c.catalog.as_str(),
        };
        coord.contains("kotlin-test") || coord.contains("junit")
    });
    if has_kotlin_test {
        keep.insert((
            crate::ops_test::JUNIT_PLATFORM_GROUP.into(),
            crate::ops_test::JUNIT_PLATFORM_STANDALONE.into(),
            crate::ops_test::JUNIT_PLATFORM_VERSION.into(),
        ));
    }

    // KSP toolchain JARs (auto-provisioned by annotation processing)
    if let Some(ref ksp_ver) = manifest.package.ksp_version {
        for coord in kargo_compiler::plugins::auto_provisioned_ksp_jars(ksp_ver, &cache) {
            keep.insert(coord);
        }
    }

    let pruned = cache.prune(&keep);

    let lock_packages = resolution_to_lockfile_packages(&result, &checksums);
    let lockfile = Lockfile::generate(lock_packages);
    lockfile.write_to(&lockfile_path)?;

    if downloaded > 0 || pruned > 0 || verbose {
        status(
            "Fetched",
            &format!(
                "{artifact_count} dependencies, {downloaded} downloaded, \
                 {up_to_date} up-to-date, {pruned} pruned"
            ),
        );
    } else if artifact_count > 0 {
        status("Fetched", &format!("all {artifact_count} dependencies up-to-date"));
    }

    Ok(())
}

/// Verify that all cached JARs match their lockfile checksums.
///
/// Reports all mismatches at once rather than failing on the first one.
pub fn verify_checksums(project_root: &Path) -> miette::Result<()> {
    let lockfile_path = project_root.join("Kargo.lock");
    let lockfile = Lockfile::from_path(&lockfile_path)?;
    let cache = LocalCache::new(project_root);
    let mut mismatches: Vec<String> = Vec::new();
    let mut verified = 0u32;
    let mut skipped = 0u32;

    for pkg in &lockfile.package {
        let expected = match &pkg.checksum {
            Some(c) if !c.is_empty() => c,
            _ => {
                skipped += 1;
                continue;
            }
        };

        let jar_path = match cache.get_jar(&pkg.group, &pkg.name, &pkg.version, None) {
            Some(p) => p,
            None => {
                skipped += 1;
                continue;
            }
        };

        let data =
            std::fs::read(&jar_path).map_err(|e| kargo_util::errors::KargoError::Generic {
                message: format!(
                    "Failed to read cached JAR {}:{}:{}: {e}",
                    pkg.group, pkg.name, pkg.version
                ),
            })?;

        let actual = sha256_bytes(&data);
        if actual == *expected {
            verified += 1;
        } else {
            mismatches.push(format!(
                "{}:{}:{}\n  expected: {expected}\n  actual:   {actual}",
                pkg.group, pkg.name, pkg.version
            ));
        }
    }

    if mismatches.is_empty() {
        kargo_util::progress::status(
            "Verified",
            &format!("{verified} checksums ({skipped} skipped, no cached JAR or no checksum)"),
        );
        Ok(())
    } else {
        let count = mismatches.len();
        let details = mismatches.join("\n");
        Err(kargo_util::errors::KargoError::Generic {
            message: format!(
                "{count} checksum mismatch(es) detected:\n{details}\n\n\
                 Cached JARs may be corrupted. Delete .kargo/dependencies and run `kargo fetch`."
            ),
        }
        .into())
    }
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

    // Include KSP and KAPT processor dependencies
    for dep in manifest.ksp.values() {
        if let Some(t) = extract(dep) {
            declared.push(t);
        }
    }
    for dep in manifest.kapt.values() {
        if let Some(t) = extract(dep) {
            declared.push(t);
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
