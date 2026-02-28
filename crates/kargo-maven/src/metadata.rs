//! maven-metadata.xml parsing for version discovery and SNAPSHOT resolution.

use quick_xml::events::Event;
use quick_xml::Reader;

/// Artifact-level Maven metadata listing available versions.
#[derive(Debug, Clone, Default)]
pub struct MavenMetadata {
    pub group_id: Option<String>,
    pub artifact_id: Option<String>,
    pub latest: Option<String>,
    pub release: Option<String>,
    pub versions: Vec<String>,
}

/// Version-level snapshot metadata for timestamped SNAPSHOT resolution.
#[derive(Debug, Clone, Default)]
pub struct SnapshotMetadata {
    pub group_id: Option<String>,
    pub artifact_id: Option<String>,
    pub version: Option<String>,
    pub timestamp: Option<String>,
    pub build_number: Option<u32>,
    pub last_updated: Option<String>,
}

impl SnapshotMetadata {
    /// Build a timestamped snapshot filename.
    ///
    /// For version `1.0-SNAPSHOT` with timestamp `20240101.120000` and build `5`,
    /// returns `artifactId-1.0-20240101.120000-5`.
    pub fn snapshot_base(&self, artifact_id: &str) -> Option<String> {
        let version = self.version.as_deref()?;
        let base = version.strip_suffix("-SNAPSHOT")?;
        let ts = self.timestamp.as_deref()?;
        let bn = self.build_number?;
        Some(format!("{artifact_id}-{base}-{ts}-{bn}"))
    }
}

/// Parse an artifact-level `maven-metadata.xml` that lists available versions.
pub fn parse_metadata(xml: &str) -> miette::Result<MavenMetadata> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut meta = MavenMetadata::default();
    let mut path: Vec<String> = Vec::new();
    let mut text_buf = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                path.push(String::from_utf8_lossy(e.name().as_ref()).to_string());
                text_buf.clear();
            }
            Ok(Event::Text(ref e)) => {
                text_buf = e.unescape().unwrap_or_default().to_string();
            }
            Ok(Event::End(_)) => {
                let ctx = path.join(">");

                match ctx.as_str() {
                    "metadata>groupId" => meta.group_id = Some(text_buf.clone()),
                    "metadata>artifactId" => meta.artifact_id = Some(text_buf.clone()),
                    "metadata>versioning>latest" => meta.latest = Some(text_buf.clone()),
                    "metadata>versioning>release" => meta.release = Some(text_buf.clone()),
                    "metadata>versioning>versions>version" => {
                        meta.versions.push(text_buf.clone());
                    }
                    _ => {}
                }

                path.pop();
                text_buf.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(kargo_util::errors::KargoError::Generic {
                    message: format!("Failed to parse maven-metadata.xml: {e}"),
                }
                .into());
            }
            _ => {}
        }
    }

    Ok(meta)
}

/// Parse a version-level `maven-metadata.xml` for SNAPSHOT timestamp resolution.
pub fn parse_snapshot_metadata(xml: &str) -> miette::Result<SnapshotMetadata> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut meta = SnapshotMetadata::default();
    let mut path: Vec<String> = Vec::new();
    let mut text_buf = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(ref e)) => {
                path.push(String::from_utf8_lossy(e.name().as_ref()).to_string());
                text_buf.clear();
            }
            Ok(Event::Text(ref e)) => {
                text_buf = e.unescape().unwrap_or_default().to_string();
            }
            Ok(Event::End(_)) => {
                let ctx = path.join(">");

                match ctx.as_str() {
                    "metadata>groupId" => meta.group_id = Some(text_buf.clone()),
                    "metadata>artifactId" => meta.artifact_id = Some(text_buf.clone()),
                    "metadata>version" => meta.version = Some(text_buf.clone()),
                    "metadata>versioning>snapshot>timestamp" => {
                        meta.timestamp = Some(text_buf.clone());
                    }
                    "metadata>versioning>snapshot>buildNumber" => {
                        meta.build_number = text_buf.parse().ok();
                    }
                    "metadata>versioning>lastUpdated" => {
                        meta.last_updated = Some(text_buf.clone());
                    }
                    _ => {}
                }

                path.pop();
                text_buf.clear();
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(kargo_util::errors::KargoError::Generic {
                    message: format!("Failed to parse snapshot metadata: {e}"),
                }
                .into());
            }
            _ => {}
        }
    }

    Ok(meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_artifact_metadata() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>org.jetbrains.kotlinx</groupId>
  <artifactId>kotlinx-coroutines-core</artifactId>
  <versioning>
    <latest>1.8.0</latest>
    <release>1.8.0</release>
    <versions>
      <version>1.6.0</version>
      <version>1.7.0</version>
      <version>1.7.3</version>
      <version>1.8.0</version>
    </versions>
    <lastUpdated>20240101120000</lastUpdated>
  </versioning>
</metadata>"#;
        let meta = parse_metadata(xml).unwrap();
        assert_eq!(meta.group_id.as_deref(), Some("org.jetbrains.kotlinx"));
        assert_eq!(meta.artifact_id.as_deref(), Some("kotlinx-coroutines-core"));
        assert_eq!(meta.latest.as_deref(), Some("1.8.0"));
        assert_eq!(meta.release.as_deref(), Some("1.8.0"));
        assert_eq!(meta.versions.len(), 4);
        assert_eq!(meta.versions[0], "1.6.0");
        assert_eq!(meta.versions[3], "1.8.0");
    }

    #[test]
    fn parse_snapshot_meta() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>com.example</groupId>
  <artifactId>my-lib</artifactId>
  <version>1.0-SNAPSHOT</version>
  <versioning>
    <snapshot>
      <timestamp>20240615.143022</timestamp>
      <buildNumber>42</buildNumber>
    </snapshot>
    <lastUpdated>20240615143022</lastUpdated>
  </versioning>
</metadata>"#;
        let meta = parse_snapshot_metadata(xml).unwrap();
        assert_eq!(meta.timestamp.as_deref(), Some("20240615.143022"));
        assert_eq!(meta.build_number, Some(42));

        let base = meta.snapshot_base("my-lib").unwrap();
        assert_eq!(base, "my-lib-1.0-20240615.143022-42");
    }

    #[test]
    fn snapshot_base_returns_none_for_non_snapshot() {
        let meta = SnapshotMetadata {
            version: Some("1.0.0".to_string()),
            timestamp: Some("20240101.000000".to_string()),
            build_number: Some(1),
            ..Default::default()
        };
        assert!(meta.snapshot_base("lib").is_none());
    }
}
