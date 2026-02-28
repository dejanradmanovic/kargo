//! Operation: display the dependency tree.

use std::path::Path;

use kargo_core::lockfile::Lockfile;
use kargo_core::manifest::Manifest;
use kargo_maven::cache::LocalCache;
use kargo_maven::download;
use kargo_resolver::resolver;

/// Options for `kargo tree`.
#[derive(Default)]
pub struct TreeOptions {
    /// Maximum tree depth to display.
    pub depth: Option<usize>,
    /// Show inverted tree for a specific dependency.
    pub why: Option<String>,
    /// Show only duplicated dependencies.
    pub duplicates: bool,
    /// Show version conflicts.
    pub conflicts: bool,
    /// Show licenses from POM metadata.
    pub licenses: bool,
    /// Show inverted tree (dependents instead of dependencies).
    pub inverted: bool,
}

/// Display the dependency tree for the project.
pub async fn tree(project_root: &Path, opts: &TreeOptions) -> miette::Result<()> {
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

    // Handle --why
    if let Some(ref target) = opts.why {
        if let Some(path) = result.graph.find_path(target) {
            println!("Path to {target}:");
            for (i, node) in path.iter().enumerate() {
                let indent = "  ".repeat(i);
                println!("{indent}{node}");
            }
        } else {
            println!("Dependency '{target}' not found in the graph.");
        }
        return Ok(());
    }

    // Handle --conflicts
    if opts.conflicts {
        if result.conflicts.is_empty() {
            println!("No version conflicts.");
        } else {
            print!("{}", result.conflicts);
        }
        return Ok(());
    }

    // Handle --licenses
    if opts.licenses {
        for artifact in &result.artifacts {
            let pom = cache.get_pom(&artifact.group, &artifact.artifact, &artifact.version);
            let license = pom
                .and_then(|p| p.licenses.first().and_then(|l| l.name.clone()))
                .unwrap_or_else(|| "Unknown".to_string());
            println!(
                "{}:{}:{} — {}",
                artifact.group, artifact.artifact, artifact.version, license
            );
        }
        return Ok(());
    }

    // Handle --duplicates
    if opts.duplicates {
        let mut found = false;
        for (key, versions) in &result.version_requests {
            if versions.len() > 1 {
                let resolved_ver = result
                    .graph
                    .find(key)
                    .map(|idx| result.graph.node(idx).version.as_str())
                    .unwrap_or("?");
                let mut vers: Vec<&str> = versions.iter().map(|s| s.as_str()).collect();
                vers.sort();
                println!(
                    "{key} (resolved {resolved_ver}) — requested: {}",
                    vers.join(", ")
                );
                found = true;
            }
        }
        if !found {
            println!("No duplicate version requests.");
        }
        return Ok(());
    }

    // Handle --inverted
    if opts.inverted {
        let inverted_output = result.graph.print_full_inverted_tree();
        if inverted_output.is_empty() {
            println!("No dependencies.");
        } else {
            print!("{inverted_output}");
        }
        return Ok(());
    }

    // Default: print tree
    let tree_output = result.graph.print_tree(opts.depth);
    print!("{tree_output}");

    Ok(())
}
