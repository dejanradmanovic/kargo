use std::path::{Path, PathBuf};

/// Walk up from `start` looking for a file named `filename`.
/// Returns the path to the directory containing the file, or `None`.
pub fn find_ancestor_with(start: &Path, filename: &str) -> Option<PathBuf> {
    let mut current = start;
    loop {
        let candidate = current.join(filename);
        if candidate.is_file() {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

/// Ensure a directory exists, creating it and any parents if needed.
pub fn ensure_dir(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

/// Recursively compute total size of a directory in bytes.
pub fn dir_size(path: &Path) -> u64 {
    std::fs::read_dir(path)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| {
                    let path = entry.path();
                    if path.is_dir() {
                        dir_size(&path)
                    } else {
                        entry.metadata().map(|m| m.len()).unwrap_or(0)
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}
