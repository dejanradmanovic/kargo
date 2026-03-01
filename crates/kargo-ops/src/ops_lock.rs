//! Operation: resolve all dependencies and regenerate Kargo.lock.

use std::collections::HashMap;
use std::path::Path;

use kargo_core::lockfile::Lockfile;
use kargo_core::manifest::Manifest;
use kargo_maven::cache::LocalCache;
use kargo_maven::download;
use kargo_resolver::resolver;
use kargo_util::hash::sha256_bytes;

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

    // Compute checksums from cached JARs if available
    let mut checksums: HashMap<String, String> = HashMap::new();
    for artifact in &result.artifacts {
        if let Some(jar_path) =
            cache.get_jar(&artifact.group, &artifact.artifact, &artifact.version, None)
        {
            if let Ok(data) = std::fs::read(&jar_path) {
                let key = format!(
                    "{}:{}:{}",
                    artifact.group, artifact.artifact, artifact.version
                );
                checksums.insert(key, sha256_bytes(&data));
            }
        }
    }

    let lock_packages = resolution_to_lockfile_packages(&result, &checksums);
    let lockfile = Lockfile::generate(lock_packages);
    let lockfile_path = project_root.join("Kargo.lock");
    lockfile.write_to(&lockfile_path)?;

    status("Resolved", &format!("{} dependencies", result.artifacts.len()));

    Ok(())
}
