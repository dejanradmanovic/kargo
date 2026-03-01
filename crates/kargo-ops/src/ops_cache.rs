//! Operation: manage the build cache, compiler metadata, and Kotlin daemon.

use kargo_compiler::build_cache::BuildCache;
use kargo_util::errors::KargoError;
use kargo_util::fs::dir_size;

/// Print cache statistics.
pub fn stats() -> miette::Result<()> {
    let cache = BuildCache::new(BuildCache::default_path(), None);
    let size = cache.size();
    let entries = cache.entry_count();

    println!("Build cache: {}", BuildCache::default_path().display());
    println!("  Entries: {entries}");
    println!("  Size:    {}", format_size(size));

    // Project-level metadata
    if let Ok(cwd) = std::env::current_dir() {
        let kargo_dir = cwd.join(".kargo");
        let fp_dir = kargo_dir.join("fingerprints");
        let deps_dir = kargo_dir.join("dependencies");

        if fp_dir.is_dir() || deps_dir.is_dir() {
            println!();
            println!("Project metadata (.kargo/):");
        }
        if deps_dir.is_dir() {
            println!("  Dependencies: {}", format_size(dir_size(&deps_dir)));
        }
        if fp_dir.is_dir() {
            println!("  Fingerprints: {}", format_size(dir_size(&fp_dir)));
        }
    }

    Ok(())
}

/// Clear the global build cache, cached dependencies, and project-level metadata.
pub fn clean() -> miette::Result<()> {
    let cache = BuildCache::new(BuildCache::default_path(), None);
    let freed = cache.clean()?;
    println!("Cleared build cache ({} freed)", format_size(freed));

    if let Ok(cwd) = std::env::current_dir() {
        let kargo_dir = cwd.join(".kargo");

        // Cached Maven dependencies (.kargo/dependencies/)
        let deps_path = kargo_dir.join("dependencies");
        if deps_path.is_dir() {
            let freed = dir_size(&deps_path);
            if let Err(e) = std::fs::remove_dir_all(&deps_path) {
                tracing::warn!(
                    "Failed to remove dependencies cache {}: {e}",
                    deps_path.display()
                );
            }
            if freed > 0 {
                println!("Cleared cached dependencies ({} freed)", format_size(freed));
            }
        }

        // Build fingerprints (.kargo/fingerprints/)
        let fp_path = kargo_dir.join("fingerprints");
        if fp_path.is_dir() {
            let freed = dir_size(&fp_path);
            if let Err(e) = std::fs::remove_dir_all(&fp_path) {
                tracing::warn!(
                    "Failed to remove fingerprints directory {}: {e}",
                    fp_path.display()
                );
            }
            if freed > 0 {
                println!("Cleared compiler metadata ({} freed)", format_size(freed));
            }
        }
    }

    Ok(())
}

/// Stop the Kotlin compiler daemon (if any).
pub fn stop_daemon() -> miette::Result<()> {
    let cwd = std::env::current_dir().map_err(KargoError::Io)?;
    let preflight = crate::ops_setup::preflight(&cwd);

    let kotlinc = match preflight {
        Ok(ref pf) => pf.toolchain.kotlinc.clone(),
        Err(_) => {
            let toolchains_dir = kargo_util::dirs_path().join("toolchains");
            let mut found = None;
            if let Ok(entries) = std::fs::read_dir(&toolchains_dir) {
                for entry in entries.flatten() {
                    let bin = entry.path().join("bin").join("kotlinc");
                    if bin.is_file() {
                        found = Some(bin);
                        break;
                    }
                }
            }
            found.ok_or_else(|| KargoError::Generic {
                message: "No Kotlin toolchain found. Nothing to stop.".into(),
            })?
        }
    };

    let daemon_client = kotlinc.with_file_name("kotlin-daemon-client");
    if daemon_client.is_file() {
        let cmd =
            kargo_util::process::CommandBuilder::new(daemon_client.to_string_lossy().to_string())
                .arg("--shutdown");
        match cmd.exec() {
            Ok(output) if output.status.success() => {
                println!("Kotlin daemon stopped.");
                return Ok(());
            }
            _ => {}
        }
    }

    println!("No Kotlin daemon was running (or it has already stopped).");
    Ok(())
}

fn format_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}
