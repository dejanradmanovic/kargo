use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::dependency::Dependency;
use crate::flavor::FlavorConfig;
use crate::profile::Profile;
use crate::target::TargetConfig;

/// The parsed representation of a `Kargo.toml` file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub package: PackageMetadata,

    #[serde(default)]
    pub targets: BTreeMap<String, TargetConfig>,

    #[serde(default)]
    pub compose: Option<ComposeConfig>,

    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,

    #[serde(default, rename = "dev-dependencies")]
    pub dev_dependencies: BTreeMap<String, Dependency>,

    #[serde(default)]
    pub target: BTreeMap<String, TargetDependencies>,

    #[serde(default)]
    pub flavor: BTreeMap<String, FlavorDependencies>,

    #[serde(default)]
    pub plugins: BTreeMap<String, PluginRef>,

    #[serde(default)]
    pub flavors: Option<FlavorConfig>,

    #[serde(default)]
    pub hooks: BTreeMap<String, Vec<String>>,

    #[serde(default)]
    pub lint: Option<LintConfig>,

    #[serde(default)]
    pub format: Option<FormatConfig>,

    #[serde(default)]
    pub profile: BTreeMap<String, Profile>,

    #[serde(default)]
    pub repositories: BTreeMap<String, RepositoryEntry>,

    #[serde(default)]
    pub workspace: Option<WorkspaceConfig>,

    #[serde(default)]
    pub toolchain: Option<ToolchainConfig>,

    #[serde(default)]
    pub catalog: Option<CatalogConfig>,

    #[serde(default)]
    pub test: Option<TestConfig>,

    #[serde(default)]
    pub signing: Option<SigningConfig>,

    #[serde(default, rename = "package.docker")]
    pub docker: Option<DockerConfig>,

    #[serde(default)]
    pub ksp: BTreeMap<String, Dependency>,

    #[serde(default, rename = "ksp-options")]
    pub ksp_options: BTreeMap<String, String>,

    #[serde(default)]
    pub kapt: BTreeMap<String, Dependency>,

    #[serde(default, rename = "kapt-options")]
    pub kapt_options: BTreeMap<String, String>,

    /// Custom compile-time constants from `[build-config]`.
    ///
    /// These are merged with flavor build-config and emitted as
    /// `const val` fields in the generated `BuildConfig` object.
    /// Values support `${env:VAR}` interpolation (resolved at load time).
    #[serde(default, rename = "build-config")]
    pub build_config: BTreeMap<String, String>,
}

/// Package identity and metadata from the `[package]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMetadata {
    pub name: String,
    #[serde(default)]
    pub group: Option<String>,
    pub version: String,
    pub kotlin: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default, rename = "main-class")]
    pub main_class: Option<String>,
    #[serde(default, rename = "ksp-version")]
    pub ksp_version: Option<String>,
}

/// Compose Multiplatform configuration from `[compose]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeConfig {
    #[serde(default)]
    pub enabled: bool,
}

/// Per-target dependency overrides from `[target.<name>.dependencies]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetDependencies {
    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,
}

/// Per-flavor dependency overrides from `[flavor.<name>.dependencies]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlavorDependencies {
    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,
}

/// A plugin reference, either a simple ID string or a detailed specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PluginRef {
    Id(String),
    Detailed { id: String, version: Option<String> },
}

/// Lint configuration from the `[lint]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintConfig {
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub severity: Option<String>,
}

/// Formatter configuration from the `[format]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormatConfig {
    #[serde(default)]
    pub style: Option<String>,
    #[serde(default)]
    pub indent: Option<u32>,
    #[serde(default, rename = "max-line-length")]
    pub max_line_length: Option<u32>,
}

/// A Maven repository reference, either a URL string or a detailed configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RepositoryEntry {
    Url(String),
    Detailed {
        url: String,
        #[serde(default)]
        auth: Option<String>,
        #[serde(default)]
        username: Option<String>,
        #[serde(default)]
        password: Option<String>,
    },
}

/// Workspace configuration from the `[workspace]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Per-project toolchain overrides from `[toolchain]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolchainConfig {
    #[serde(default)]
    pub jdk: Option<String>,
    #[serde(default, rename = "kotlin-mirror")]
    pub kotlin_mirror: Option<String>,
    #[serde(default, rename = "auto-download")]
    pub auto_download: Option<bool>,
}

/// Version catalog configuration from `[catalog]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogConfig {
    #[serde(default)]
    pub versions: BTreeMap<String, String>,
    #[serde(default)]
    pub libraries: BTreeMap<String, CatalogLibrary>,
    #[serde(default)]
    pub bundles: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub plugins: BTreeMap<String, CatalogPlugin>,
}

/// A library entry in the version catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogLibrary {
    pub group: String,
    pub artifact: String,
    #[serde(default, rename = "version.ref")]
    pub version_ref: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

/// A plugin entry in the version catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogPlugin {
    pub id: String,
    #[serde(default, rename = "version.ref")]
    pub version_ref: Option<String>,
}

/// Test configuration from `[test]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    #[serde(default)]
    pub coverage: Option<CoverageConfig>,
}

/// Code coverage configuration from `[test.coverage]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageConfig {
    #[serde(default)]
    pub engine: Option<String>,
    #[serde(default, rename = "min-line")]
    pub min_line: Option<u32>,
    #[serde(default, rename = "min-branch")]
    pub min_branch: Option<u32>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Artifact signing configuration from `[signing]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningConfig {
    #[serde(default, rename = "gpg-key")]
    pub gpg_key: Option<String>,
    #[serde(default, rename = "gpg-password")]
    pub gpg_password: Option<String>,
}

/// Docker packaging configuration from `[package.docker]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerConfig {
    #[serde(default, rename = "base-image")]
    pub base_image: Option<String>,
    #[serde(default)]
    pub ports: Vec<u16>,
    #[serde(default)]
    pub entrypoint: Option<String>,
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

impl Manifest {
    /// Load and parse a `Kargo.toml` file from the given path.
    ///
    /// Before parsing, `${env:VAR}` references in the manifest content are
    /// resolved using `.kargo.env` (if present alongside `Kargo.toml`) and
    /// process environment variables.
    pub fn from_path(path: &Path) -> miette::Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            kargo_util::errors::KargoError::Manifest {
                message: format!("Failed to read {}: {e}", path.display()),
            }
        })?;

        let dir = path.parent().unwrap_or(Path::new("."));
        let env_vars =
            crate::properties::load_env_file(&dir.join(".kargo.env")).unwrap_or_default();
        let resolved = crate::properties::interpolate(&content, &env_vars);

        let manifest = Self::parse_toml(&resolved)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Parse a `Kargo.toml` from a string (no interpolation).
    pub fn parse_toml(content: &str) -> miette::Result<Self> {
        toml::from_str(content).map_err(|e| {
            kargo_util::errors::KargoError::Manifest {
                message: format!("Failed to parse Kargo.toml: {e}"),
            }
            .into()
        })
    }

    /// Validate semantic constraints that TOML deserialization cannot enforce.
    pub fn validate(&self) -> miette::Result<()> {
        use kargo_util::errors::KargoError;
        let err = |msg: String| -> miette::Report { KargoError::Manifest { message: msg }.into() };

        // name: non-empty, valid identifier chars
        if self.package.name.is_empty() {
            return Err(err("package.name must not be empty".into()));
        }
        if !self
            .package
            .name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(err(format!(
                "package.name '{}' contains invalid characters (allowed: alphanumeric, '-', '_')",
                self.package.name
            )));
        }

        // version: valid semver
        if semver::Version::parse(&self.package.version).is_err() {
            return Err(err(format!(
                "package.version '{}' is not valid semver (expected e.g. 1.0.0)",
                self.package.version
            )));
        }

        // kotlin: major.minor or major.minor.patch
        if !is_valid_kotlin_version(&self.package.kotlin) {
            return Err(err(format!(
                "package.kotlin '{}' is not a valid version (expected e.g. 2.3.0 or 2.3)",
                self.package.kotlin
            )));
        }

        // group: if present, valid Maven group (dot-separated identifiers)
        if let Some(ref group) = self.package.group {
            if group.is_empty()
                || !group
                    .split('.')
                    .all(|seg| !seg.is_empty() && seg.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-'))
            {
                return Err(err(format!(
                    "package.group '{}' is not a valid Maven group ID (expected dot-separated identifiers)",
                    group
                )));
            }
        }

        // repository: if present, valid URL
        if let Some(ref repo) = self.package.repository {
            if !repo.starts_with("http://") && !repo.starts_with("https://") {
                return Err(err(format!(
                    "package.repository '{}' must start with http:// or https://",
                    repo
                )));
            }
        }

        // Duplicate dependency detection across sections
        let mut seen = std::collections::HashSet::new();
        let sections: &[(&str, &BTreeMap<String, Dependency>)] = &[
            ("dependencies", &self.dependencies),
            ("dev-dependencies", &self.dev_dependencies),
            ("ksp", &self.ksp),
            ("kapt", &self.kapt),
        ];
        for (section, deps) in sections {
            for key in deps.keys() {
                if !seen.insert(key.clone()) {
                    return Err(err(format!(
                        "duplicate dependency '{}' found in [{}] (already declared in another section)",
                        key, section
                    )));
                }
            }
        }

        Ok(())
    }
}

fn is_valid_kotlin_version(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    matches!(parts.len(), 2 | 3)
        && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}
