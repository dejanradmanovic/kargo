use std::path::PathBuf;

use crate::package::Package;

/// A Kargo workspace: a root manifest with member packages.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub root_dir: PathBuf,
    pub members: Vec<Package>,
}

impl Workspace {
    /// Returns `true` if this workspace has multiple members or if the sole member is in a subdirectory.
    pub fn is_virtual(&self) -> bool {
        self.members.len() > 1
            || self
                .members
                .first()
                .map(|m| m.root_dir != self.root_dir)
                .unwrap_or(true)
    }
}
