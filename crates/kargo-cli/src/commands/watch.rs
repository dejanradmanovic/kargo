//! Watch command: rebuild (and optionally run) on file changes.
//!
//! By default, `kargo watch` builds and runs the project on every change.
//! Pass `--build-only` to only build without running.
//!
//! Uses `notify` to watch source directories, `Kargo.toml`, and resource
//! directories. Events are debounced so rapid saves (e.g. from an IDE)
//! trigger a single cycle.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use miette::Result;
use notify::{RecursiveMode, Watcher};

use kargo_core::manifest::Manifest;
use kargo_ops::ops_build::{self, BuildOptions};
use kargo_util::errors::KargoError;

const DEBOUNCE_MS: u64 = 300;

pub fn exec(build_only: bool, verbose: bool) -> Result<()> {
    let cwd = std::env::current_dir().map_err(KargoError::Io)?;
    let mode = if build_only { "build" } else { "build + run" };

    let watch_paths = collect_watch_paths(&cwd)?;

    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            if is_relevant_event(&event) {
                let _ = tx.send(());
            }
        }
    })
    .map_err(|e| KargoError::Generic {
        message: format!("Failed to create file watcher: {e}"),
    })?;

    for path in &watch_paths {
        if path.is_dir() {
            watcher
                .watch(path, RecursiveMode::Recursive)
                .map_err(|e| KargoError::Generic {
                    message: format!("Failed to watch {}: {e}", path.display()),
                })?;
        } else if path.is_file() {
            watcher
                .watch(path, RecursiveMode::NonRecursive)
                .map_err(|e| KargoError::Generic {
                    message: format!("Failed to watch {}: {e}", path.display()),
                })?;
        }
    }

    kargo_util::progress::status("Watching", &format!("for changes (mode: {mode})"));
    if verbose {
        for p in &watch_paths {
            eprintln!("  watching: {}", p.display());
        }
    }

    // Initial cycle
    run_cycle(&cwd, build_only, verbose);

    // Watch loop
    loop {
        match rx.recv() {
            Ok(()) => {}
            Err(_) => break,
        }

        // Debounce: drain additional events within the window
        std::thread::sleep(Duration::from_millis(DEBOUNCE_MS));
        while rx.try_recv().is_ok() {}

        eprint!("\x1B[2J\x1B[H");
        kargo_util::progress::status("Detected", "change, rebuilding...");
        run_cycle(&cwd, build_only, verbose);
    }

    Ok(())
}

fn run_cycle(cwd: &Path, build_only: bool, verbose: bool) {
    let build_result = ops_build::build(
        cwd,
        &BuildOptions {
            verbose,
            ..Default::default()
        },
    );

    match build_result {
        Ok(result) if result.success && !build_only => {
            if let Err(e) = kargo_ops::ops_run::run(cwd, None, &[], verbose) {
                kargo_util::progress::status_warn("Error", &format!("{e}"));
            }
            kargo_util::progress::status("Watching", "for changes...");
        }
        Ok(_) => {
            kargo_util::progress::status("Watching", "for changes...");
        }
        Err(e) => {
            kargo_util::progress::status_warn("Error", &format!("{e}"));
            kargo_util::progress::status("Watching", "for changes...");
        }
    }
}

/// Collect all paths that should be watched for changes.
fn collect_watch_paths(project_dir: &Path) -> Result<Vec<PathBuf>> {
    let manifest = Manifest::from_path(&project_dir.join("Kargo.toml"))?;
    let discovered =
        kargo_compiler::source_set_discovery::discover(project_dir, &manifest);

    let mut paths = Vec::new();

    // Source and resource directories
    for ss in discovered
        .main_sources
        .iter()
        .chain(discovered.test_sources.iter())
    {
        for dir in &ss.kotlin_dirs {
            if dir.is_dir() {
                paths.push(dir.clone());
            }
        }
        for dir in &ss.resource_dirs {
            if dir.is_dir() {
                paths.push(dir.clone());
            }
        }
    }

    // Manifest file
    let manifest_path = project_dir.join("Kargo.toml");
    if manifest_path.is_file() {
        paths.push(manifest_path);
    }

    // Dedup (source sets may share roots)
    paths.sort();
    paths.dedup();

    // Remove paths that are children of other watched paths
    let mut pruned = Vec::new();
    for path in &paths {
        let is_child = pruned.iter().any(|parent: &PathBuf| {
            path.starts_with(parent) && path != parent
        });
        if !is_child {
            pruned.push(path.clone());
        }
    }

    Ok(pruned)
}

/// Filter out events that are unlikely to be meaningful source changes.
fn is_relevant_event(event: &notify::Event) -> bool {
    use notify::EventKind;

    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {}
        _ => return false,
    }

    event.paths.iter().any(|p| {
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Skip build output, hidden files, and editor temps
        if name.starts_with('.') || name.ends_with('~') || name.ends_with(".swp") {
            return false;
        }
        if p.components().any(|c| {
            let s = c.as_os_str().to_string_lossy();
            s == "build" || s == ".kargo" || s == "target"
        }) {
            return false;
        }

        matches!(ext, "kt" | "java" | "toml" | "properties" | "xml" | "json" | "yaml" | "yml")
            || name == "Kargo.toml"
    })
}
