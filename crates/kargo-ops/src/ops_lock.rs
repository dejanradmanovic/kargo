//! Operation: resolve all dependencies and regenerate Kargo.lock.

use std::collections::HashMap;
use std::path::Path;

use kargo_core::lockfile::Lockfile;
use kargo_core::manifest::Manifest;
use kargo_maven::cache::LocalCache;
use kargo_maven::download;
use kargo_resolver::resolver;

use crate::ops_fetch::resolution_to_lockfile_packages;

/// Force re-resolve all dependencies and regenerate `Kargo.lock`.
pub async fn lock(project_root: &Path, verbose: bool) -> miette::Result<()> {
    use kargo_util::progress::{spinner, status};

    let manifest_path = project_root.join("Kargo.toml");
    let manifest = Manifest::from_path(&manifest_path)?;
    let repos = resolver::build_repos(&manifest);
    let cache = LocalCache::new(project_root);

    let sp = spinner("Resolving dependencies...");
    let client = download::build_client()?;

    // Force fresh resolution (no lockfile fast-path)
    let result = resolver::resolve(&manifest, &repos, &cache, None, &client).await?;
    sp.finish_and_clear();

    if !result.conflicts.is_empty() && verbose {
        eprintln!("{}", result.conflicts);
    }

    // Compute checksums from cached JARs in parallel
    let jar_entries: Vec<_> = result
        .artifacts
        .iter()
        .filter_map(|artifact| {
            cache
                .get_jar(&artifact.group, &artifact.artifact, &artifact.version, None)
                .map(|jar_path| {
                    let key = format!(
                        "{}:{}:{}",
                        artifact.group, artifact.artifact, artifact.version
                    );
                    (key, jar_path)
                })
        })
        .collect();

    let checksums: HashMap<String, String> = std::thread::scope(|s| {
        let handles: Vec<_> = jar_entries
            .iter()
            .map(|(key, jar_path)| {
                let key = key.clone();
                let jar_path = jar_path.clone();
                s.spawn(move || {
                    kargo_util::hash::sha256_file_streaming(&jar_path)
                        .ok()
                        .map(|hash| (key, hash))
                })
            })
            .collect();

        handles
            .into_iter()
            .filter_map(|h| h.join().unwrap())
            .collect()
    });

    let lock_packages = resolution_to_lockfile_packages(&result, &checksums);
    let lockfile = Lockfile::generate(lock_packages);
    let lockfile_path = project_root.join("Kargo.lock");
    lockfile.write_to(&lockfile_path)?;

    status(
        "Resolved",
        &format!("{} dependencies", result.artifacts.len()),
    );

    Ok(())
}
