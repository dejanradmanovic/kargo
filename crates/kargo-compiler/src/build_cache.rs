//! Content-addressable build cache: local LRU cache.
//!
//! Stores compiled output keyed by the build fingerprint hash.
//! On a cache hit, compiled artifacts are restored from the cache
//! instead of recompiling.

use std::fs;
use std::path::{Path, PathBuf};

use kargo_util::errors::KargoError;

use crate::fingerprint::Fingerprint;

/// Local build cache backed by the filesystem.
pub struct BuildCache {
    root: PathBuf,
    max_bytes: u64,
}

impl BuildCache {
    /// Create a build cache at the default or configured location.
    ///
    /// `root` is typically `~/.kargo/build-cache/`.
    pub fn new(root: PathBuf, max_size_str: Option<&str>) -> Self {
        let max_bytes = parse_size(max_size_str.unwrap_or("10GB"));
        Self { root, max_bytes }
    }

    /// Default cache path: `~/.kargo/build-cache/`.
    pub fn default_path() -> PathBuf {
        kargo_util::dirs_path().join("build-cache")
    }

    /// Check if a cached build exists for the given fingerprint.
    pub fn get(&self, fp: &Fingerprint) -> Option<PathBuf> {
        let entry_dir = self.entry_dir(fp);
        if entry_dir.is_dir() {
            // Touch the marker to update LRU order
            let marker = entry_dir.join(".kargo-cache-marker");
            let _ = fs::write(&marker, chrono_now());
            Some(entry_dir)
        } else {
            None
        }
    }

    /// Store build output in the cache under the fingerprint key.
    ///
    /// Copies the contents of `classes_dir` into the cache.
    pub fn put(&self, fp: &Fingerprint, classes_dir: &Path) -> miette::Result<()> {
        let entry_dir = self.entry_dir(fp);
        if entry_dir.exists() {
            let _ = fs::remove_dir_all(&entry_dir);
        }
        copy_dir_recursive(classes_dir, &entry_dir)?;

        let marker = entry_dir.join(".kargo-cache-marker");
        let _ = fs::write(&marker, chrono_now());

        self.evict_if_needed();
        Ok(())
    }

    /// Restore cached artifacts to the target output directory.
    pub fn restore(&self, fp: &Fingerprint, target_dir: &Path) -> miette::Result<bool> {
        let entry_dir = match self.get(fp) {
            Some(d) => d,
            None => return Ok(false),
        };
        copy_dir_recursive(&entry_dir, target_dir)?;
        // Remove the cache marker from restored output
        let _ = fs::remove_file(target_dir.join(".kargo-cache-marker"));
        Ok(true)
    }

    /// Total size of the cache in bytes.
    pub fn size(&self) -> u64 {
        dir_size(&self.root)
    }

    /// Number of cached entries.
    pub fn entry_count(&self) -> u32 {
        if !self.root.is_dir() {
            return 0;
        }
        fs::read_dir(&self.root)
            .map(|rd| rd.flatten().filter(|e| e.path().is_dir()).count() as u32)
            .unwrap_or(0)
    }

    /// Remove all cached entries.
    pub fn clean(&self) -> miette::Result<u64> {
        let size = self.size();
        if self.root.is_dir() {
            fs::remove_dir_all(&self.root).map_err(KargoError::Io)?;
        }
        Ok(size)
    }

    fn entry_dir(&self, fp: &Fingerprint) -> PathBuf {
        self.root.join(&fp.hash)
    }

    fn evict_if_needed(&self) {
        if self.size() <= self.max_bytes {
            return;
        }

        // Collect entries with their last-access time
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };

        let mut dirs: Vec<(PathBuf, u64)> = entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| {
                let marker = e.path().join(".kargo-cache-marker");
                let ts = fs::read_to_string(&marker)
                    .ok()
                    .and_then(|s| s.trim().parse::<u64>().ok())
                    .unwrap_or(0);
                (e.path(), ts)
            })
            .collect();

        // Sort oldest first
        dirs.sort_by_key(|(_, ts)| *ts);

        let mut current_size = self.size();
        for (dir, _) in &dirs {
            if current_size <= self.max_bytes {
                break;
            }
            let entry_size = dir_size(dir);
            let _ = fs::remove_dir_all(dir);
            current_size = current_size.saturating_sub(entry_size);
        }
    }
}

fn chrono_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

fn parse_size(s: &str) -> u64 {
    let s = s.trim();
    let (num, unit) = if s.ends_with("GB") {
        (s.trim_end_matches("GB").trim(), 1024u64 * 1024 * 1024)
    } else if s.ends_with("MB") {
        (s.trim_end_matches("MB").trim(), 1024u64 * 1024)
    } else if s.ends_with("KB") {
        (s.trim_end_matches("KB").trim(), 1024u64)
    } else {
        (s, 1u64)
    };
    num.parse::<u64>().unwrap_or(10) * unit
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> miette::Result<()> {
    fs::create_dir_all(dst).map_err(KargoError::Io)?;
    let entries = fs::read_dir(src).map_err(KargoError::Io)?;
    for entry in entries.flatten() {
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &dest)?;
        } else {
            fs::copy(&path, &dest).map_err(KargoError::Io)?;
        }
    }
    Ok(())
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(m) = entry.metadata() {
                if m.is_dir() {
                    total += dir_size(&entry.path());
                } else {
                    total += m.len();
                }
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_size_values() {
        assert_eq!(parse_size("10GB"), 10 * 1024 * 1024 * 1024);
        assert_eq!(parse_size("500MB"), 500 * 1024 * 1024);
        assert_eq!(parse_size("1024"), 1024);
    }

    #[test]
    fn cache_put_and_restore() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = BuildCache::new(tmp.path().join("cache"), None);
        let fp = Fingerprint {
            hash: "abc123".into(),
        };

        let src_dir = tmp.path().join("classes");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("Main.class"), b"bytecode").unwrap();

        cache.put(&fp, &src_dir).unwrap();
        assert!(cache.get(&fp).is_some());

        let restore_dir = tmp.path().join("restored");
        assert!(cache.restore(&fp, &restore_dir).unwrap());
        assert!(restore_dir.join("Main.class").is_file());
    }
}
