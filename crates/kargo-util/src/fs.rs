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
