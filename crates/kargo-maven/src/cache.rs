//! Project-local Maven cache management mirroring repository layout.

use std::fs;
use std::path::{Path, PathBuf};

use crate::pom::{self, Pom};
use crate::repository::MavenRepository;

/// Project-local Maven artifact cache at `<project>/.kargo/dependencies/`.
#[derive(Debug, Clone)]
pub struct LocalCache {
    root: PathBuf,
}

impl LocalCache {
    /// Create a cache rooted at `project_root/.kargo/dependencies/`.
    pub fn new(project_root: &Path) -> Self {
        Self {
            root: project_root.join(".kargo").join("dependencies"),
        }
    }

    /// The root directory of this cache.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Path within the cache for a given Maven coordinate.
    pub fn artifact_dir(&self, group: &str, artifact: &str, version: &str) -> PathBuf {
        self.root
            .join(group.replace('.', "/"))
            .join(artifact)
            .join(version)
    }

    /// Path to a specific file in the cache.
    fn artifact_path(&self, group: &str, artifact: &str, version: &str, filename: &str) -> PathBuf {
        self.artifact_dir(group, artifact, version).join(filename)
    }

    /// Check if a JAR is cached and return its path.
    pub fn get_jar(
        &self,
        group: &str,
        artifact: &str,
        version: &str,
        classifier: Option<&str>,
    ) -> Option<PathBuf> {
        let filename = match classifier {
            Some(c) => format!("{artifact}-{version}-{c}.jar"),
            None => format!("{artifact}-{version}.jar"),
        };
        let path = self.artifact_path(group, artifact, version, &filename);
        path.is_file().then_some(path)
    }

    /// Check if a POM is cached and parse it.
    pub fn get_pom(&self, group: &str, artifact: &str, version: &str) -> Option<Pom> {
        let filename = format!("{artifact}-{version}.pom");
        let path = self.artifact_path(group, artifact, version, &filename);
        if !path.is_file() {
            return None;
        }
        let content = fs::read_to_string(&path).ok()?;
        pom::parse_pom(&content).ok()
    }

    /// Store artifact data in the cache, creating directories as needed.
    pub fn put(
        &self,
        group: &str,
        artifact: &str,
        version: &str,
        filename: &str,
        data: &[u8],
    ) -> miette::Result<PathBuf> {
        let dir = self.artifact_dir(group, artifact, version);
        fs::create_dir_all(&dir).map_err(kargo_util::errors::KargoError::Io)?;
        let path = dir.join(filename);
        fs::write(&path, data).map_err(kargo_util::errors::KargoError::Io)?;
        Ok(path)
    }

    /// Store a POM file in the cache.
    pub fn put_pom(
        &self,
        group: &str,
        artifact: &str,
        version: &str,
        pom_xml: &str,
    ) -> miette::Result<PathBuf> {
        let filename = format!("{artifact}-{version}.pom");
        self.put(group, artifact, version, &filename, pom_xml.as_bytes())
    }

    /// Store a JAR file in the cache.
    pub fn put_jar(
        &self,
        group: &str,
        artifact: &str,
        version: &str,
        classifier: Option<&str>,
        data: &[u8],
    ) -> miette::Result<PathBuf> {
        let filename = match classifier {
            Some(c) => format!("{artifact}-{version}-{c}.jar"),
            None => format!("{artifact}-{version}.jar"),
        };
        self.put(group, artifact, version, &filename, data)
    }

    /// Check whether the JAR for this coordinate exists in cache.
    pub fn has_artifact(&self, group: &str, artifact: &str, version: &str) -> bool {
        self.get_jar(group, artifact, version, None).is_some()
    }

    /// Fetch or download a POM, using cache when available.
    pub async fn fetch_pom(
        &self,
        client: &reqwest::Client,
        repo: &MavenRepository,
        group: &str,
        artifact: &str,
        version: &str,
    ) -> miette::Result<Option<Pom>> {
        if let Some(pom) = self.get_pom(group, artifact, version) {
            return Ok(Some(pom));
        }

        let url = repo.pom_url(group, artifact, version);
        let xml = crate::download::download_text(client, repo, &url).await?;
        match xml {
            Some(content) => {
                self.put_pom(group, artifact, version, &content)?;
                let pom = pom::parse_pom(&content)?;
                Ok(Some(pom))
            }
            None => Ok(None),
        }
    }

    /// Remove cached artifacts not present in the resolved set.
    ///
    /// `keep` contains `(group, artifact, version)` tuples of artifacts
    /// that should be retained. Everything else gets deleted.
    /// Returns the number of version directories removed.
    pub fn prune(&self, keep: &std::collections::HashSet<(String, String, String)>) -> u32 {
        let mut removed = 0u32;
        if !self.root.is_dir() {
            return removed;
        }
        collect_version_dirs(&self.root, &self.root, keep, &mut removed);
        removed
    }

    /// Total size of the cache directory in bytes.
    pub fn size(&self) -> u64 {
        dir_size(&self.root)
    }
}

/// Walk the cache tree to find version directories (leaf dirs containing files)
/// and remove those not in the `keep` set.
///
/// Cache layout: `<root>/<group-path>/<artifact>/<version>/`
/// We reconstruct the coordinate by tracking depth from root.
fn collect_version_dirs(
    root: &Path,
    current: &Path,
    keep: &std::collections::HashSet<(String, String, String)>,
    removed: &mut u32,
) {
    let Ok(entries) = fs::read_dir(current) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let has_files = fs::read_dir(&path)
            .map(|rd| rd.flatten().any(|e| e.path().is_file()))
            .unwrap_or(false);

        if has_files {
            // This is a version dir: reconstruct group:artifact:version from path
            if let Some(coord) = reconstruct_coordinate(root, &path) {
                if !keep.contains(&coord) {
                    let _ = fs::remove_dir_all(&path);
                    *removed += 1;
                }
            }
        } else {
            collect_version_dirs(root, &path, keep, removed);
            // Remove empty parent dirs after pruning children
            if fs::read_dir(&path)
                .map(|mut rd| rd.next().is_none())
                .unwrap_or(true)
            {
                let _ = fs::remove_dir(&path);
            }
        }
    }
}

/// Reconstruct `(group, artifact, version)` from a cache path.
///
/// Path: `<root>/org/jetbrains/kotlin/kotlin-stdlib/2.3.0`
/// Result: `("org.jetbrains.kotlin", "kotlin-stdlib", "2.3.0")`
fn reconstruct_coordinate(root: &Path, version_dir: &Path) -> Option<(String, String, String)> {
    let rel = version_dir.strip_prefix(root).ok()?;
    let components: Vec<_> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    if components.len() < 3 {
        return None;
    }
    let version = components.last()?.clone();
    let artifact = components[components.len() - 2].clone();
    let group = components[..components.len() - 2].join(".");
    Some((group, artifact, version))
}

fn dir_size(path: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let meta = entry.metadata();
            if let Ok(m) = meta {
                if m.is_dir() {
                    total += dir_size(&entry.path());
                } else {
                    total += m.len();
                }
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_put_and_get() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = LocalCache::new(tmp.path());

        cache
            .put_jar("org.example", "lib", "1.0", None, b"fake jar data")
            .unwrap();

        let path = cache.get_jar("org.example", "lib", "1.0", None);
        assert!(path.is_some());
        let content = std::fs::read(path.unwrap()).unwrap();
        assert_eq!(content, b"fake jar data");
    }

    #[test]
    fn cache_pom_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = LocalCache::new(tmp.path());

        let pom_xml = r#"<?xml version="1.0"?>
<project>
  <groupId>org.example</groupId>
  <artifactId>lib</artifactId>
  <version>1.0</version>
</project>"#;

        cache.put_pom("org.example", "lib", "1.0", pom_xml).unwrap();
        let pom = cache.get_pom("org.example", "lib", "1.0");
        assert!(pom.is_some());
        assert_eq!(pom.unwrap().artifact_id.as_deref(), Some("lib"));
    }

    #[test]
    fn cache_miss() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = LocalCache::new(tmp.path());
        assert!(cache.get_jar("com.missing", "lib", "1.0", None).is_none());
        assert!(!cache.has_artifact("com.missing", "lib", "1.0"));
    }

    #[test]
    fn cache_layout_mirrors_maven() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = LocalCache::new(tmp.path());
        cache
            .put(
                "org.jetbrains.kotlin",
                "kotlin-stdlib",
                "2.3.0",
                "kotlin-stdlib-2.3.0.jar",
                b"x",
            )
            .unwrap();

        let expected = tmp.path().join(
            ".kargo/dependencies/org/jetbrains/kotlin/kotlin-stdlib/2.3.0/kotlin-stdlib-2.3.0.jar",
        );
        assert!(expected.is_file());
    }

    #[test]
    fn prune_removes_stale_artifacts() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = LocalCache::new(tmp.path());

        cache
            .put_jar("org.example", "lib", "1.0", None, b"old jar")
            .unwrap();
        cache
            .put_jar("org.example", "lib", "2.0", None, b"new jar")
            .unwrap();
        cache
            .put_jar("org.other", "util", "3.0", None, b"keep")
            .unwrap();

        assert!(cache.has_artifact("org.example", "lib", "1.0"));
        assert!(cache.has_artifact("org.example", "lib", "2.0"));
        assert!(cache.has_artifact("org.other", "util", "3.0"));

        let mut keep = std::collections::HashSet::new();
        keep.insert(("org.example".into(), "lib".into(), "2.0".into()));
        keep.insert(("org.other".into(), "util".into(), "3.0".into()));

        let pruned = cache.prune(&keep);
        assert_eq!(pruned, 1);

        assert!(!cache.has_artifact("org.example", "lib", "1.0"));
        assert!(cache.has_artifact("org.example", "lib", "2.0"));
        assert!(cache.has_artifact("org.other", "util", "3.0"));
    }

    #[test]
    fn prune_cleans_empty_parent_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = LocalCache::new(tmp.path());

        cache
            .put_jar("org.removed", "gone", "1.0", None, b"data")
            .unwrap();

        let keep = std::collections::HashSet::new();
        let pruned = cache.prune(&keep);
        assert_eq!(pruned, 1);

        // The entire org/removed/gone directory tree should be gone
        assert!(!cache.artifact_dir("org.removed", "gone", "1.0").exists());
    }
}
