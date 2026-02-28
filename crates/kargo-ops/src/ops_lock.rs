//! Operation: resolve all dependencies and regenerate Kargo.lock.

use std::path::Path;

use kargo_core::lockfile::Lockfile;
use kargo_core::manifest::Manifest;
use kargo_maven::cache::LocalCache;
use kargo_maven::download;
use kargo_resolver::resolver;

use crate::ops_fetch::resolution_to_lockfile_packages;

/// Force re-resolve all dependencies and regenerate `Kargo.lock`.
pub async fn lock(project_root: &Path, verbose: bool) -> miette::Result<()> {
    let manifest_path = project_root.join("Kargo.toml");
    let manifest = Manifest::from_path(&manifest_path)?;
    let repos = resolver::build_repos(&manifest);
    let cache = LocalCache::new(project_root);

    let client = download::build_client()?;

    // Force fresh resolution (no lockfile fast-path)
    let result = resolver::resolve(&manifest, &repos, &cache, None, &client).await?;

    if !result.conflicts.is_empty() && verbose {
        eprintln!("{}", result.conflicts);
    }

    let lock_packages = resolution_to_lockfile_packages(&result);
    let lockfile = Lockfile::generate(lock_packages);
    let lockfile_path = project_root.join("Kargo.lock");
    lockfile.write_to(&lockfile_path)?;

    eprintln!("Resolved {} dependencies", result.artifacts.len());

    Ok(())
}
