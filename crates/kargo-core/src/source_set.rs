use std::collections::BTreeSet;
use std::path::PathBuf;

/// Represents a Kotlin source set (e.g., commonMain, jvmMain, freeMain).
#[derive(Debug, Clone)]
pub struct SourceSet {
    pub name: String,
    pub kotlin_dirs: Vec<PathBuf>,
    pub resource_dirs: Vec<PathBuf>,
    pub depends_on: BTreeSet<String>,
}

impl SourceSet {
    /// Create a new source set with conventional directory paths under `base_dir`.
    pub fn new(name: impl Into<String>, base_dir: PathBuf) -> Self {
        let name = name.into();
        let kotlin_dir = base_dir.join(&name).join("kotlin");
        let resource_dir = base_dir.join(&name).join("resources");
        Self {
            name,
            kotlin_dirs: vec![kotlin_dir],
            resource_dirs: vec![resource_dir],
            depends_on: BTreeSet::new(),
        }
    }

    /// Add a dependency on another source set (builder pattern).
    pub fn with_depends_on(mut self, parent: impl Into<String>) -> Self {
        self.depends_on.insert(parent.into());
        self
    }

    /// Returns `true` if any of the Kotlin source directories exist on disk.
    pub fn exists(&self) -> bool {
        self.kotlin_dirs.iter().any(|d| d.is_dir())
    }
}
