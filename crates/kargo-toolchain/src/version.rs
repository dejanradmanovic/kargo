//! Kotlin version parsing and management.

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use semver::Version;
use serde::{Deserialize, Serialize};

/// A parsed Kotlin compiler version backed by semver.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KotlinVersion(Version);

impl KotlinVersion {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self(Version::new(major, minor, patch))
    }

    /// Read the `kotlin` field from a `Kargo.toml` at the given path.
    pub fn from_manifest(path: &Path) -> miette::Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            kargo_util::errors::KargoError::Toolchain {
                message: format!("Failed to read {}: {e}", path.display()),
            }
        })?;

        #[derive(Deserialize)]
        struct Partial {
            package: PartialPackage,
        }
        #[derive(Deserialize)]
        struct PartialPackage {
            kotlin: String,
        }

        let parsed: Partial = toml::from_str(&content).map_err(|e| {
            kargo_util::errors::KargoError::Toolchain {
                message: format!("Failed to parse {}: {e}", path.display()),
            }
        })?;

        Self::from_str(&parsed.package.kotlin).map_err(|e| {
            kargo_util::errors::KargoError::Toolchain {
                message: format!("Invalid kotlin version '{}': {e}", parsed.package.kotlin),
            }
            .into()
        })
    }

    pub fn major(&self) -> u64 {
        self.0.major
    }

    pub fn minor(&self) -> u64 {
        self.0.minor
    }

    pub fn patch(&self) -> u64 {
        self.0.patch
    }
}

impl fmt::Display for KotlinVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for KotlinVersion {
    type Err = semver::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Version::parse(s).map(Self)
    }
}

impl From<Version> for KotlinVersion {
    fn from(v: Version) -> Self {
        Self(v)
    }
}
