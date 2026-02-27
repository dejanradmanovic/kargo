use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Global user configuration loaded from `~/.kargo/config.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default)]
    pub build: BuildConfig,

    #[serde(default)]
    pub repositories: BTreeMap<String, String>,

    #[serde(default)]
    pub credentials: BTreeMap<String, CredentialEntry>,

    #[serde(default)]
    pub cache: CacheConfig,

    #[serde(default)]
    pub toolchain: ToolchainGlobalConfig,

    #[serde(default)]
    pub lint: Option<GlobalLintConfig>,

    #[serde(default)]
    pub format: Option<GlobalFormatConfig>,
}

/// Build settings from `[build]` in global config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    #[serde(default = "default_jobs")]
    pub jobs: u32,
    #[serde(default, rename = "default-target")]
    pub default_target: Option<String>,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            jobs: default_jobs(),
            default_target: None,
        }
    }
}

fn default_jobs() -> u32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(4)
}

/// Credential entry for a named repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialEntry {
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default, rename = "token-cmd")]
    pub token_cmd: Option<String>,
}

/// Build cache configuration from `[cache]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_cache_dir")]
    pub dir: String,
    #[serde(default, rename = "max-size")]
    pub max_size: Option<String>,
    #[serde(default)]
    pub remote: Option<String>,
    #[serde(default, rename = "remote-auth")]
    pub remote_auth: Option<String>,
    #[serde(default, rename = "remote-push")]
    pub remote_push: Option<bool>,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            dir: default_cache_dir(),
            max_size: None,
            remote: None,
            remote_auth: None,
            remote_push: None,
        }
    }
}

fn default_cache_dir() -> String {
    "~/.kargo/cache".to_string()
}

/// Global toolchain settings from `[toolchain]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolchainGlobalConfig {
    #[serde(default, rename = "kotlin-mirror")]
    pub kotlin_mirror: Option<String>,
    #[serde(default = "default_auto_download", rename = "auto-download")]
    pub auto_download: bool,
    #[serde(default)]
    pub jdk: Option<String>,
}

impl Default for ToolchainGlobalConfig {
    fn default() -> Self {
        Self {
            kotlin_mirror: None,
            auto_download: true,
            jdk: None,
        }
    }
}

fn default_auto_download() -> bool {
    true
}

/// Global lint defaults from `[lint]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalLintConfig {
    #[serde(default, rename = "default-rules")]
    pub default_rules: Vec<String>,
}

/// Global format defaults from `[format]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalFormatConfig {
    #[serde(default)]
    pub style: Option<String>,
}

impl GlobalConfig {
    /// Load the global configuration from `~/.kargo/config.toml`, or return defaults if the file doesn't exist.
    pub fn load() -> miette::Result<Self> {
        let path = Self::default_path();
        if path.is_file() {
            let content = std::fs::read_to_string(&path).map_err(|e| {
                kargo_util::errors::KargoError::Generic {
                    message: format!("Failed to read global config: {e}"),
                }
            })?;
            toml::from_str(&content).map_err(|e| {
                kargo_util::errors::KargoError::Generic {
                    message: format!("Failed to parse global config: {e}"),
                }
                .into()
            })
        } else {
            Ok(Self::default())
        }
    }

    /// Returns the default path to the global config file.
    pub fn default_path() -> PathBuf {
        dirs_path().join("config.toml")
    }
}

/// Returns the path to the Kargo data directory (`~/.kargo/`).
pub fn dirs_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    Path::new(&home).join(".kargo")
}
