use serde::{Deserialize, Serialize};

/// A dependency specification in Kargo.toml.
///
/// Supports both shorthand (`"group:artifact:version"`) and detailed forms.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    Short(String),
    Detailed(DetailedDependency),
    Catalog(CatalogDependency),
}

/// A dependency with explicit group, artifact, version, and optional metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedDependency {
    pub group: String,
    pub artifact: String,
    pub version: String,
    #[serde(default)]
    pub scope: Option<DependencyScope>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub exclusions: Vec<Exclusion>,
    #[serde(default)]
    pub classifier: Option<String>,
}

/// A reference to a version catalog entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogDependency {
    pub catalog: String,
    #[serde(default)]
    pub bundle: bool,
}

/// A transitive dependency to exclude.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exclusion {
    pub group: String,
    #[serde(default)]
    pub artifact: Option<String>,
}

/// Maven-compatible dependency scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyScope {
    Compile,
    Runtime,
    Provided,
    Test,
}

impl Default for DependencyScope {
    fn default() -> Self {
        Self::Compile
    }
}

/// Maven coordinates parsed from a shorthand string.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MavenCoordinate {
    pub group_id: String,
    pub artifact_id: String,
    pub version: String,
}

impl MavenCoordinate {
    /// Parse `"group:artifact:version"` into coordinates.
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() == 3 {
            Some(Self {
                group_id: parts[0].to_string(),
                artifact_id: parts[1].to_string(),
                version: parts[2].to_string(),
            })
        } else {
            None
        }
    }
}

impl std::fmt::Display for MavenCoordinate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.group_id, self.artifact_id, self.version)
    }
}
