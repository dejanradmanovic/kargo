//! Content-addressable build cache: local LRU cache.
//!
//! Stores compiled output keyed by the build fingerprint hash.
//! On a cache hit, compiled artifacts are restored from the cache
//! instead of recompiling.
//!
//! Size is tracked incrementally in a `.kargo-cache-size` metadata
//! file to avoid repeated full-tree walks during eviction.

use std::fs;
use std::path::{Path, PathBuf};

use kargo_util::errors::KargoError;
use kargo_util::fs::dir_size as util_dir_size;

use crate::fingerprint::Fingerprint;

const SIZE_FILE: &str = ".kargo-cache-size";

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
            let marker = entry_dir.join(".kargo-cache-marker");
            if let Err(e) = fs::write(&marker, chrono_now()) {
                tracing::warn!("Failed to update cache marker {}: {e}", marker.display());
            }
            Some(entry_dir)
        } else {
            None
        }
    }

    /// Store build output in the cache under the fingerprint key.
    ///
    /// Copies the contents of `classes_dir` into the cache and updates
    /// the tracked total size incrementally.
    pub fn put(&self, fp: &Fingerprint, classes_dir: &Path) -> miette::Result<()> {
        let entry_dir = self.entry_dir(fp);

        // If replacing an existing entry, subtract its size first
        if entry_dir.exists() {
            let old_size = dir_size(&entry_dir);
            if let Err(e) = fs::remove_dir_all(&entry_dir) {
                tracing::warn!("Failed to remove cache entry {}: {e}", entry_dir.display());
            }
            self.adjust_tracked_size(-(old_size as i64));
        }

        copy_dir_recursive(classes_dir, &entry_dir)?;

        let marker = entry_dir.join(".kargo-cache-marker");
        if let Err(e) = fs::write(&marker, chrono_now()) {
            tracing::warn!("Failed to write cache marker {}: {e}", marker.display());
        }

        let new_size = dir_size(&entry_dir);
        self.adjust_tracked_size(new_size as i64);

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
        if let Err(e) = fs::remove_file(target_dir.join(".kargo-cache-marker")) {
            tracing::warn!(
                "Failed to remove cache marker from {}: {e}",
                target_dir.display()
            );
        }
        Ok(true)
    }

    /// Total size of the cache in bytes (read from tracked metadata,
    /// with a full-walk fallback if the metadata is missing or corrupted).
    pub fn size(&self) -> u64 {
        self.read_tracked_size().unwrap_or_else(|| {
            let actual = dir_size(&self.root);
            self.write_tracked_size(actual);
            actual
        })
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

    /// Rebuild the tracked size by walking the full cache tree.
    /// Use for recovery if the size file is suspected to be inaccurate.
    pub fn rebuild_size(&self) -> u64 {
        let actual = dir_size(&self.root);
        self.write_tracked_size(actual);
        actual
    }

    fn entry_dir(&self, fp: &Fingerprint) -> PathBuf {
        self.root.join(&fp.hash)
    }

    fn size_file_path(&self) -> PathBuf {
        self.root.join(SIZE_FILE)
    }

    fn read_tracked_size(&self) -> Option<u64> {
        let path = self.size_file_path();
        fs::read_to_string(path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
    }

    fn write_tracked_size(&self, size: u64) {
        if let Err(e) = fs::create_dir_all(&self.root) {
            tracing::warn!(
                "Failed to create build cache root {}: {e}",
                self.root.display()
            );
        }
        if let Err(e) = fs::write(self.size_file_path(), size.to_string()) {
            tracing::warn!(
                "Failed to write cache size file {}: {e}",
                self.size_file_path().display()
            );
        }
    }

    fn adjust_tracked_size(&self, delta: i64) {
        let current = self.read_tracked_size().unwrap_or(0);
        let new_size = if delta >= 0 {
            current.saturating_add(delta as u64)
        } else {
            current.saturating_sub((-delta) as u64)
        };
        self.write_tracked_size(new_size);
    }

    fn evict_if_needed(&self) {
        let mut current_size = self.size();
        if current_size <= self.max_bytes {
            return;
        }

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

        // Sort oldest first (LRU eviction)
        dirs.sort_by_key(|(_, ts)| *ts);

        for (dir, _) in &dirs {
            if current_size <= self.max_bytes {
                break;
            }
            let entry_size = dir_size(dir);
            if let Err(e) = fs::remove_dir_all(dir) {
                tracing::warn!("Failed to evict cache entry {}: {e}", dir.display());
            }
            current_size = current_size.saturating_sub(entry_size);
        }

        // Sync the tracked size after eviction
        self.write_tracked_size(current_size);
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

/// Directory size excluding the SIZE_FILE metadata (used for cache tracking).
fn dir_size(path: &Path) -> u64 {
    let total = util_dir_size(path);
    let size_file = path.join(SIZE_FILE);
    if size_file.is_file() {
        total.saturating_sub(fs::metadata(&size_file).map(|m| m.len()).unwrap_or(0))
    } else {
        total
    }
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

    #[test]
    fn incremental_size_tracking() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = BuildCache::new(tmp.path().join("cache"), None);

        let src_dir = tmp.path().join("classes");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("Main.class"), b"bytecode").unwrap();

        let fp = Fingerprint {
            hash: "size_test".into(),
        };
        cache.put(&fp, &src_dir).unwrap();

        let tracked = cache.read_tracked_size().unwrap();
        assert!(tracked > 0);

        let actual = dir_size(&cache.root);
        assert_eq!(tracked, actual);
    }

    #[test]
    fn rebuild_size_recovers() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = BuildCache::new(tmp.path().join("cache"), None);

        let src_dir = tmp.path().join("classes");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("A.class"), b"aaaa").unwrap();

        let fp = Fingerprint {
            hash: "rebuild".into(),
        };
        cache.put(&fp, &src_dir).unwrap();

        // Corrupt the size file
        cache.write_tracked_size(999999);
        assert_eq!(cache.read_tracked_size(), Some(999999));

        // Rebuild should fix it
        let correct = cache.rebuild_size();
        assert!(correct < 999999);
        assert_eq!(cache.read_tracked_size(), Some(correct));
    }
}
