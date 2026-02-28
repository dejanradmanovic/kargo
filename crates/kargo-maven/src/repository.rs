//! Maven repository abstraction: URL layout, source types, configuration.

use kargo_core::manifest::RepositoryEntry;

/// Maven Central base URL.
pub const MAVEN_CENTRAL_URL: &str = "https://repo.maven.apache.org/maven2";

/// Google's Maven repository.
pub const GOOGLE_MAVEN_URL: &str = "https://maven.google.com";

/// A configured Maven repository with optional credentials.
#[derive(Debug, Clone)]
pub struct MavenRepository {
    pub name: String,
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
}

impl MavenRepository {
    /// Build a `MavenRepository` from a name and a manifest `RepositoryEntry`.
    pub fn from_entry(name: &str, entry: &RepositoryEntry) -> Self {
        match entry {
            RepositoryEntry::Url(url) => Self {
                name: name.to_string(),
                url: url.trim_end_matches('/').to_string(),
                username: None,
                password: None,
            },
            RepositoryEntry::Detailed {
                url,
                username,
                password,
                ..
            } => Self {
                name: name.to_string(),
                url: url.trim_end_matches('/').to_string(),
                username: username.clone(),
                password: password.clone(),
            },
        }
    }

    /// Construct the default Maven Central repository.
    pub fn maven_central() -> Self {
        Self {
            name: "maven-central".to_string(),
            url: MAVEN_CENTRAL_URL.to_string(),
            username: None,
            password: None,
        }
    }

    /// Construct the Google Maven repository.
    pub fn google() -> Self {
        Self {
            name: "google".to_string(),
            url: GOOGLE_MAVEN_URL.to_string(),
            username: None,
            password: None,
        }
    }

    /// Standard Maven layout path for a given coordinate.
    ///
    /// `org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0` becomes
    /// `org/jetbrains/kotlinx/kotlinx-coroutines-core/1.8.0`
    pub fn coordinate_path(group: &str, artifact: &str, version: &str) -> String {
        format!("{}/{}/{}", group.replace('.', "/"), artifact, version)
    }

    /// Full URL to a specific file within the Maven repository.
    pub fn file_url(&self, group: &str, artifact: &str, version: &str, filename: &str) -> String {
        format!(
            "{}/{}/{}",
            self.url,
            Self::coordinate_path(group, artifact, version),
            filename
        )
    }

    /// URL to the POM file for a given coordinate.
    pub fn pom_url(&self, group: &str, artifact: &str, version: &str) -> String {
        let filename = format!("{artifact}-{version}.pom");
        self.file_url(group, artifact, version, &filename)
    }

    /// URL to the JAR file for a given coordinate.
    pub fn jar_url(
        &self,
        group: &str,
        artifact: &str,
        version: &str,
        classifier: Option<&str>,
    ) -> String {
        let filename = match classifier {
            Some(c) => format!("{artifact}-{version}-{c}.jar"),
            None => format!("{artifact}-{version}.jar"),
        };
        self.file_url(group, artifact, version, &filename)
    }

    /// URL to the `maven-metadata.xml` at the artifact level (version listing).
    pub fn metadata_url(&self, group: &str, artifact: &str) -> String {
        format!(
            "{}/{}/{}/maven-metadata.xml",
            self.url,
            group.replace('.', "/"),
            artifact
        )
    }

    /// URL to the `maven-metadata.xml` at the version level (SNAPSHOT resolution).
    pub fn snapshot_metadata_url(&self, group: &str, artifact: &str, version: &str) -> String {
        format!(
            "{}/{}/maven-metadata.xml",
            self.url,
            Self::coordinate_path(group, artifact, version)
        )
    }

    /// URL to a Gradle Module Metadata `.module` file.
    pub fn module_url(&self, group: &str, artifact: &str, version: &str) -> String {
        let filename = format!("{artifact}-{version}.module");
        self.file_url(group, artifact, version, &filename)
    }

    /// Whether this repository has authentication configured.
    pub fn has_auth(&self) -> bool {
        self.username.is_some() || self.password.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coordinate_path_replaces_dots() {
        let path = MavenRepository::coordinate_path(
            "org.jetbrains.kotlinx",
            "kotlinx-coroutines-core",
            "1.8.0",
        );
        assert_eq!(path, "org/jetbrains/kotlinx/kotlinx-coroutines-core/1.8.0");
    }

    #[test]
    fn pom_url_format() {
        let repo = MavenRepository::maven_central();
        let url = repo.pom_url("org.jetbrains.kotlinx", "kotlinx-coroutines-core", "1.8.0");
        assert_eq!(
            url,
            "https://repo.maven.apache.org/maven2/org/jetbrains/kotlinx/kotlinx-coroutines-core/1.8.0/kotlinx-coroutines-core-1.8.0.pom"
        );
    }

    #[test]
    fn jar_url_with_classifier() {
        let repo = MavenRepository::maven_central();
        let url = repo.jar_url("com.example", "my-lib", "1.0", Some("sources"));
        assert!(url.ends_with("my-lib-1.0-sources.jar"));
    }

    #[test]
    fn jar_url_without_classifier() {
        let repo = MavenRepository::maven_central();
        let url = repo.jar_url("com.example", "my-lib", "1.0", None);
        assert!(url.ends_with("my-lib-1.0.jar"));
    }

    #[test]
    fn metadata_url_format() {
        let repo = MavenRepository::maven_central();
        let url = repo.metadata_url("org.jetbrains.kotlinx", "kotlinx-coroutines-core");
        assert_eq!(
            url,
            "https://repo.maven.apache.org/maven2/org/jetbrains/kotlinx/kotlinx-coroutines-core/maven-metadata.xml"
        );
    }

    #[test]
    fn from_entry_url() {
        let entry = RepositoryEntry::Url("https://repo.example.com/maven/".to_string());
        let repo = MavenRepository::from_entry("test", &entry);
        assert_eq!(repo.url, "https://repo.example.com/maven");
        assert!(!repo.has_auth());
    }

    #[test]
    fn from_entry_detailed_with_auth() {
        let entry = RepositoryEntry::Detailed {
            url: "https://nexus.co/maven".to_string(),
            auth: None,
            username: Some("user".to_string()),
            password: Some("pass".to_string()),
        };
        let repo = MavenRepository::from_entry("nexus", &entry);
        assert!(repo.has_auth());
        assert_eq!(repo.username.as_deref(), Some("user"));
    }
}
