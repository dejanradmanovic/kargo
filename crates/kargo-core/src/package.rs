use std::path::PathBuf;

use crate::manifest::Manifest;

/// A resolved Kargo package: one `Kargo.toml` manifest plus its source sets.
#[derive(Debug, Clone)]
pub struct Package {
    pub manifest: Manifest,
    pub manifest_path: PathBuf,
    pub root_dir: PathBuf,
}

impl Package {
    /// Returns the package name from the manifest.
    pub fn name(&self) -> &str {
        &self.manifest.package.name
    }

    /// Returns the package version from the manifest.
    pub fn version(&self) -> &str {
        &self.manifest.package.version
    }

    /// Returns the Kotlin compiler version from the manifest.
    pub fn kotlin_version(&self) -> &str {
        &self.manifest.package.kotlin
    }

    /// Returns the path to the source directory.
    pub fn src_dir(&self) -> PathBuf {
        self.root_dir.join("src")
    }

    /// Returns the path to the build output directory.
    pub fn build_dir(&self) -> PathBuf {
        self.root_dir.join("build")
    }
}
