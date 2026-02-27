use serde::{Deserialize, Serialize};

/// Build profile configuration controlling debug/optimization settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default)]
    pub debug: Option<bool>,

    #[serde(default)]
    pub optimization: Option<bool>,

    #[serde(default, rename = "compiler-args")]
    pub compiler_args: Vec<String>,
}

impl Profile {
    /// Create the default development profile (debug enabled, no optimization).
    pub fn dev() -> Self {
        Self {
            debug: Some(true),
            optimization: Some(false),
            compiler_args: Vec::new(),
        }
    }

    /// Create the default release profile (no debug, optimization enabled).
    pub fn release() -> Self {
        Self {
            debug: Some(false),
            optimization: Some(true),
            compiler_args: Vec::new(),
        }
    }
}
