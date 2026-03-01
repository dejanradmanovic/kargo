//! Build fingerprinting for incremental compilation decisions.
//!
//! Two-tier strategy:
//! 1. **Fast mtime check** — compare the newest source file's modification time
//!    against a stored timestamp. If nothing is newer, skip the expensive hash.
//! 2. **Full SHA-256 hash** — deterministic hash of all compilation inputs
//!    (source contents, dependency versions, compiler args, Kotlin version, profile).
//!
//! Fingerprint data is stored under `.kargo/fingerprints/` (project-level) so
//! that the `build/` directory contains only compilation output.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use sha2::{Digest, Sha256};

use crate::unit::CompilationUnit;

/// A computed build fingerprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    pub hash: String,
}

/// Derive the fingerprint storage directory for a given project, target, and profile.
///
/// Layout: `<project>/.kargo/fingerprints/<target>/<profile>/`
pub fn storage_dir(project_dir: &Path, target: &str, profile: &str) -> PathBuf {
    project_dir
        .join(".kargo")
        .join("fingerprints")
        .join(target)
        .join(profile)
}

/// Compute a fingerprint for a compilation unit.
pub fn compute(unit: &CompilationUnit, kotlin_version: &str) -> Fingerprint {
    let mut hasher = Sha256::new();

    hasher.update(b"unit:");
    hasher.update(unit.name.as_bytes());
    hasher.update(b"\n");

    hasher.update(b"kotlin:");
    hasher.update(kotlin_version.as_bytes());
    hasher.update(b"\n");

    hasher.update(b"target:");
    hasher.update(unit.target.kebab_name().as_bytes());
    hasher.update(b"\n");

    hasher.update(b"test:");
    hasher.update(if unit.is_test {
        "true".as_bytes()
    } else {
        "false".as_bytes()
    });
    hasher.update(b"\n");

    // Compiler args
    for arg in &unit.compiler_args {
        hasher.update(b"arg:");
        hasher.update(arg.as_bytes());
        hasher.update(b"\n");
    }

    // Classpath JAR filenames (not contents — too expensive)
    let mut cp: Vec<String> = unit
        .classpath
        .iter()
        .filter_map(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
        .collect();
    cp.sort();
    for jar in &cp {
        hasher.update(b"cp:");
        hasher.update(jar.as_bytes());
        hasher.update(b"\n");
    }

    // Source file contents
    let mut all_sources = unit.all_sources();
    all_sources.sort();
    for src in &all_sources {
        if let Ok(content) = std::fs::read(src) {
            hasher.update(b"src:");
            hasher.update(src.to_string_lossy().as_bytes());
            hasher.update(b":");
            let file_hash = Sha256::digest(&content);
            hasher.update(format!("{file_hash:x}").as_bytes());
            hasher.update(b"\n");
        }
    }

    // Processor JAR hashes — changing a processor version triggers rebuild
    let mut proc_jars: Vec<String> = unit
        .processor_jars
        .iter()
        .filter_map(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
        .collect();
    proc_jars.sort();
    for jar in &proc_jars {
        hasher.update(b"proc:");
        hasher.update(jar.as_bytes());
        hasher.update(b"\n");
    }

    let result = hasher.finalize();
    Fingerprint {
        hash: format!("{result:x}"),
    }
}

// ---------------------------------------------------------------------------
// Mtime fast-path
// ---------------------------------------------------------------------------

/// Compute the maximum modification time across all source files in a unit.
/// Returns epoch seconds, or 0 if no files have metadata.
pub fn max_mtime(unit: &CompilationUnit) -> u64 {
    let mut max = 0u64;
    for src in &unit.sources {
        if let Ok(meta) = std::fs::metadata(src) {
            if let Ok(modified) = meta.modified() {
                let secs = modified
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                if secs > max {
                    max = secs;
                }
            }
        }
    }
    for dir in &unit.generated_sources {
        max = max.max(dir_max_mtime(dir));
    }
    max
}

fn dir_max_mtime(dir: &Path) -> u64 {
    let mut max = 0u64;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            max = max.max(dir_max_mtime(&path));
        } else if let Ok(meta) = path.metadata() {
            if let Ok(modified) = meta.modified() {
                let secs = modified
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                max = max.max(secs);
            }
        }
    }
    max
}

/// Path where the mtime marker is stored for a given unit.
fn mtime_path(fp_dir: &Path, unit_name: &str) -> PathBuf {
    fp_dir.join(format!("{unit_name}.mtime"))
}

/// Load the previously stored mtime and source count for a unit.
pub fn load_mtime(fp_dir: &Path, unit_name: &str) -> Option<(u64, usize)> {
    let path = mtime_path(fp_dir, unit_name);
    let content = std::fs::read_to_string(path).ok()?;
    let trimmed = content.trim();
    if let Some((mtime_str, count_str)) = trimmed.split_once(' ') {
        let mtime = mtime_str.parse().ok()?;
        let count = count_str.parse().ok()?;
        Some((mtime, count))
    } else {
        // Backward compat: old markers without a count
        let mtime = trimmed.parse().ok()?;
        Some((mtime, 0))
    }
}

/// Save the mtime marker and source count after a successful compilation.
pub fn save_mtime(
    fp_dir: &Path,
    unit_name: &str,
    mtime: u64,
    source_count: usize,
) -> miette::Result<()> {
    let path = mtime_path(fp_dir, unit_name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(kargo_util::errors::KargoError::Io)?;
    }
    std::fs::write(&path, format!("{mtime} {source_count}")).map_err(|e| {
        kargo_util::errors::KargoError::Generic {
            message: format!("Failed to write mtime marker: {e}"),
        }
        .into()
    })
}

// ---------------------------------------------------------------------------
// Full fingerprint persistence
// ---------------------------------------------------------------------------

/// Path where a fingerprint is stored for a given unit.
pub fn fingerprint_path(fp_dir: &Path, unit_name: &str) -> PathBuf {
    fp_dir.join(format!("{unit_name}.txt"))
}

/// Load a previously stored fingerprint, if it exists.
pub fn load(fp_dir: &Path, unit_name: &str) -> Option<Fingerprint> {
    let path = fingerprint_path(fp_dir, unit_name);
    std::fs::read_to_string(path).ok().map(|hash| Fingerprint {
        hash: hash.trim().to_string(),
    })
}

/// Save a fingerprint to disk.
pub fn save(fp_dir: &Path, unit_name: &str, fp: &Fingerprint) -> miette::Result<()> {
    let path = fingerprint_path(fp_dir, unit_name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(kargo_util::errors::KargoError::Io)?;
    }
    std::fs::write(&path, &fp.hash).map_err(|e| {
        kargo_util::errors::KargoError::Generic {
            message: format!("Failed to write fingerprint: {e}"),
        }
        .into()
    })
}
