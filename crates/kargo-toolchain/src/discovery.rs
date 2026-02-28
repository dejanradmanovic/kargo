//! Toolchain discovery: find installed Kotlin compilers and resolve binary paths.

use std::path::PathBuf;

use kargo_util::errors::KargoError;

use crate::install;
use crate::version::KotlinVersion;

/// Paths to compiler binaries within a managed Kotlin toolchain.
#[derive(Debug, Clone)]
pub struct ToolchainPaths {
    pub home: PathBuf,
    pub version: KotlinVersion,
    pub kotlinc: PathBuf,
    pub kotlinc_jvm: PathBuf,
    pub kotlin_native: Option<PathBuf>,
}

impl ToolchainPaths {
    /// Build paths from a toolchain root directory.
    fn from_home(home: PathBuf, version: KotlinVersion) -> Self {
        let bin = home.join("bin");
        let kotlinc = bin.join("kotlinc");
        let kotlinc_jvm = kotlinc.clone();

        // kotlin-native ships as a separate download; it may or may not
        // be present inside the standard compiler distribution.
        let native_candidate = bin.join("kotlinc-native");
        let kotlin_native = if native_candidate.exists() {
            Some(native_candidate)
        } else {
            None
        };

        Self {
            home,
            version,
            kotlinc,
            kotlinc_jvm,
            kotlin_native,
        }
    }
}

/// Resolve a toolchain for the given version.
///
/// 1. If the version is already installed under `~/.kargo/toolchains/`, use it.
/// 2. If `auto_download` is true, download and install it.
/// 3. Otherwise return an error.
pub fn resolve_toolchain(
    version: &KotlinVersion,
    auto_download: bool,
    mirror: Option<&str>,
) -> miette::Result<ToolchainPaths> {
    let home = install::toolchain_dir(version);

    if home.is_dir() {
        return Ok(ToolchainPaths::from_home(home, version.clone()));
    }

    if auto_download {
        let installed_home = install::install_kotlin(version, mirror)?;
        return Ok(ToolchainPaths::from_home(installed_home, version.clone()));
    }

    Err(KargoError::Toolchain {
        message: format!(
            "Kotlin {version} is not installed. Run `kargo toolchain install {version}`"
        ),
    }
    .into())
}

/// Resolve the toolchain for the current project.
///
/// Reads `Kargo.toml` in `project_dir` (or its ancestors) to find the
/// required Kotlin version, then resolves the toolchain.
pub fn resolve_project_toolchain(
    project_dir: &std::path::Path,
    auto_download: bool,
    mirror: Option<&str>,
) -> miette::Result<ToolchainPaths> {
    let manifest_path = kargo_util::fs::find_ancestor_with(project_dir, "Kargo.toml")
        .map(|d| d.join("Kargo.toml"))
        .ok_or_else(|| KargoError::Toolchain {
            message: "No Kargo.toml found in this directory or any parent".to_string(),
        })?;

    let version = KotlinVersion::from_manifest(&manifest_path)?;
    resolve_toolchain(&version, auto_download, mirror)
}
