use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Flavor configuration from `[flavors]`, defining flavor dimensions and their values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlavorConfig {
    pub dimensions: Vec<String>,

    #[serde(default)]
    pub default: Option<BTreeMap<String, String>>,

    #[serde(default)]
    pub exclude: Vec<BTreeMap<String, String>>,

    #[serde(flatten)]
    pub dimension_flavors: BTreeMap<String, BTreeMap<String, FlavorDefinition>>,
}

/// A single flavor's configuration within a dimension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlavorDefinition {
    #[serde(default, rename = "build-config")]
    pub build_config: BTreeMap<String, String>,

    #[serde(default, rename = "application-id-suffix")]
    pub application_id_suffix: Option<String>,

    #[serde(default)]
    pub signing: Option<FlavorSigning>,
}

/// Flavor-specific signing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlavorSigning {
    pub keystore: Option<String>,
    #[serde(rename = "key-alias")]
    pub key_alias: Option<String>,
    #[serde(rename = "store-password-cmd")]
    pub store_password_cmd: Option<String>,
}

/// A resolved build variant: one flavor per dimension + one profile.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BuildVariant {
    /// Map from dimension name to selected flavor name.
    pub flavors: BTreeMap<String, String>,
    pub profile: String,
}

impl BuildVariant {
    /// Returns the variant name as a kebab-case string (e.g., `free-staging-release`).
    pub fn name(&self) -> String {
        let mut parts: Vec<&str> = self.flavors.values().map(|s| s.as_str()).collect();
        parts.push(&self.profile);
        parts.join("-")
    }

    /// Returns the variant name in camelCase (e.g., `freeStagingRelease`).
    pub fn camel_case_name(&self) -> String {
        let mut result = String::new();
        for (i, part) in self
            .flavors
            .values()
            .chain(std::iter::once(&self.profile))
            .enumerate()
        {
            if i == 0 {
                result.push_str(part);
            } else {
                let mut chars = part.chars();
                if let Some(first) = chars.next() {
                    result.extend(first.to_uppercase());
                    result.extend(chars);
                }
            }
        }
        result
    }
}
