//! Incremental compilation support: change detection and partial recompilation.
//!
//! Two-tier strategy for deciding whether to recompile:
//! 1. **Mtime pre-check** — if no source file is newer than the stored mtime,
//!    the unit is definitely up-to-date without reading file contents.
//! 2. **Full fingerprint** — if mtime detects a potential change, compute the
//!    SHA-256 fingerprint and compare against the stored one.
//!
//! Fingerprint data lives in `.kargo/fingerprints/<target>/<profile>/` so it
//! does not pollute the `build/` output directory.

use std::path::Path;

use crate::fingerprint::{self, Fingerprint};
use crate::unit::CompilationUnit;

/// Result of an incremental check.
#[derive(Debug)]
pub enum IncrementalDecision {
    /// Inputs haven't changed; skip compilation.
    UpToDate,
    /// Inputs have changed; recompile. The new fingerprint should be saved after success.
    NeedsRebuild(Fingerprint),
}

/// Check whether a compilation unit needs to be rebuilt.
///
/// `fp_dir` is the fingerprint storage directory
/// (typically `.kargo/fingerprints/<target>/<profile>/`).
pub fn check(unit: &CompilationUnit, fp_dir: &Path, kotlin_version: &str) -> IncrementalDecision {
    // If output directory doesn't exist or is empty, definitely rebuild.
    // The directory may have been re-created (e.g. by BuildContext) after
    // a clean, so an empty dir must also trigger a rebuild.
    if !unit.output_dir.is_dir() || dir_is_empty(&unit.output_dir) {
        let fp = fingerprint::compute(unit, kotlin_version);
        return IncrementalDecision::NeedsRebuild(fp);
    }

    // Fast path: mtime + source count comparison.
    // Removing a file doesn't increase max mtime, so we also check that the
    // number of source files hasn't changed.
    let current_mtime = fingerprint::max_mtime(unit);
    let current_count = unit.sources.len();
    if let Some((stored_mtime, stored_count)) = fingerprint::load_mtime(fp_dir, &unit.name) {
        let count_matches = stored_count == 0 || current_count == stored_count;
        if current_mtime <= stored_mtime
            && count_matches
            && fingerprint::load(fp_dir, &unit.name).is_some()
        {
            return IncrementalDecision::UpToDate;
        }
    }

    // Slow path: full content-based fingerprint
    let current = fingerprint::compute(unit, kotlin_version);

    match fingerprint::load(fp_dir, &unit.name) {
        Some(stored) if stored == current => {
            // Content hasn't actually changed (e.g. file was touched but not modified).
            let _ = fingerprint::save_mtime(fp_dir, &unit.name, current_mtime, current_count);
            IncrementalDecision::UpToDate
        }
        _ => IncrementalDecision::NeedsRebuild(current),
    }
}

/// Save the fingerprint and mtime marker after a successful compilation.
pub fn mark_complete(
    fp_dir: &Path,
    unit_name: &str,
    fp: &Fingerprint,
    unit: &CompilationUnit,
) -> miette::Result<()> {
    fingerprint::save(fp_dir, unit_name, fp)?;
    let mtime = fingerprint::max_mtime(unit);
    fingerprint::save_mtime(fp_dir, unit_name, mtime, unit.sources.len())?;
    Ok(())
}

fn dir_is_empty(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .map(|mut rd| rd.next().is_none())
        .unwrap_or(true)
}
