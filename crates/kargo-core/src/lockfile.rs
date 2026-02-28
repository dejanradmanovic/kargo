use serde::{Deserialize, Serialize};
use std::path::Path;

/// Deterministic lockfile recording exact resolved dependency versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    #[serde(default)]
    pub package: Vec<LockedPackage>,
}

/// A single locked dependency with its resolved coordinates and checksum.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedPackage {
    pub name: String,
    pub group: String,
    pub version: String,
    #[serde(default)]
    pub checksum: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<LockedDependencyRef>,
}

/// A reference to a transitive dependency within the lockfile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedDependencyRef {
    pub name: String,
    pub group: String,
    pub version: String,
}

impl Lockfile {
    /// Load and parse a `Kargo.lock` file from the given path.
    pub fn from_path(path: &Path) -> miette::Result<Self> {
        let content =
            std::fs::read_to_string(path).map_err(|e| kargo_util::errors::KargoError::Generic {
                message: format!("Failed to read lockfile: {e}"),
            })?;
        toml::from_str(&content).map_err(|e| {
            kargo_util::errors::KargoError::Generic {
                message: format!("Failed to parse lockfile: {e}"),
            }
            .into()
        })
    }

    /// Serialize the lockfile to a pretty-printed TOML string.
    pub fn to_string_pretty(&self) -> Result<String, toml::ser::Error> {
        toml::to_string_pretty(self)
    }
}
