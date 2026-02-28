//! Operation: add a dependency to Kargo.toml.

use std::path::Path;

use kargo_core::dependency::MavenCoordinate;
use toml_edit::{DocumentMut, Item, Table, Value};

/// Options for `kargo add`.
pub struct AddOptions {
    /// The dependency spec: `group:artifact:version`.
    pub spec: String,
    /// Add as a dev dependency.
    pub dev: bool,
    /// Add to a specific target section.
    pub target: Option<String>,
    /// Add to a specific flavor section.
    pub flavor: Option<String>,
}

/// Add a dependency to `Kargo.toml` using format-preserving edits.
pub fn add_dependency(manifest_path: &Path, opts: &AddOptions) -> miette::Result<()> {
    let content = std::fs::read_to_string(manifest_path).map_err(|e| {
        kargo_util::errors::KargoError::Manifest {
            message: format!("Failed to read {}: {e}", manifest_path.display()),
        }
    })?;

    let mut doc: DocumentMut =
        content
            .parse()
            .map_err(|e| kargo_util::errors::KargoError::Manifest {
                message: format!("Failed to parse Kargo.toml: {e}"),
            })?;

    let coord = MavenCoordinate::parse(&opts.spec).ok_or_else(|| {
        kargo_util::errors::KargoError::Generic {
            message: format!(
                "Invalid dependency format: '{}'. Expected group:artifact:version",
                opts.spec
            ),
        }
    })?;

    let dep_key = coord.artifact_id.clone();
    let dep_value = coord.to_string();

    if let Some(ref target) = opts.target {
        ensure_table(&mut doc, &["target", target, "dependencies"]);
        doc["target"][target]["dependencies"][&dep_key] =
            Item::Value(Value::from(dep_value.clone()));
    } else if let Some(ref flavor) = opts.flavor {
        ensure_table(&mut doc, &["flavor", flavor, "dependencies"]);
        doc["flavor"][flavor]["dependencies"][&dep_key] =
            Item::Value(Value::from(dep_value.clone()));
    } else if opts.dev {
        ensure_table(&mut doc, &["dev-dependencies"]);
        doc["dev-dependencies"][&dep_key] = Item::Value(Value::from(dep_value.clone()));
    } else {
        ensure_table(&mut doc, &["dependencies"]);
        doc["dependencies"][&dep_key] = Item::Value(Value::from(dep_value.clone()));
    }

    std::fs::write(manifest_path, doc.to_string())
        .map_err(|e| kargo_util::errors::KargoError::Io(e).into())
}

/// Ensure a nested table path exists in the document.
fn ensure_table(doc: &mut DocumentMut, keys: &[&str]) {
    let mut current = doc.as_table_mut() as &mut Table;
    for &key in keys {
        if !current.contains_key(key) {
            current.insert(key, Item::Table(Table::new()));
        }
        current = match current.get_mut(key) {
            Some(Item::Table(t)) => t,
            _ => return,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_to_dependencies() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Kargo.toml");
        std::fs::write(
            &path,
            r#"[package]
name = "test"
version = "0.1.0"
kotlin = "2.3.0"

[dependencies]
"#,
        )
        .unwrap();

        add_dependency(
            &path,
            &AddOptions {
                spec: "org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0".to_string(),
                dev: false,
                target: None,
                flavor: None,
            },
        )
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("kotlinx-coroutines-core"));
        assert!(content.contains("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0"));
    }

    #[test]
    fn add_dev_dependency() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Kargo.toml");
        std::fs::write(
            &path,
            r#"[package]
name = "test"
version = "0.1.0"
kotlin = "2.3.0"
"#,
        )
        .unwrap();

        add_dependency(
            &path,
            &AddOptions {
                spec: "junit:junit:4.13.2".to_string(),
                dev: true,
                target: None,
                flavor: None,
            },
        )
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("[dev-dependencies]"));
        assert!(content.contains("junit"));
    }

    #[test]
    fn add_target_dependency() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Kargo.toml");
        std::fs::write(
            &path,
            r#"[package]
name = "test"
version = "0.1.0"
kotlin = "2.3.0"
"#,
        )
        .unwrap();

        add_dependency(
            &path,
            &AddOptions {
                spec: "org.example:jvm-lib:1.0".to_string(),
                dev: false,
                target: Some("jvm".to_string()),
                flavor: None,
            },
        )
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("[target.jvm.dependencies]"));
        assert!(content.contains("jvm-lib"));
    }

    #[test]
    fn invalid_spec() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Kargo.toml");
        std::fs::write(
            &path,
            r#"[package]
name = "test"
version = "0.1.0"
kotlin = "2.3.0"
"#,
        )
        .unwrap();

        let result = add_dependency(
            &path,
            &AddOptions {
                spec: "invalid-spec".to_string(),
                dev: false,
                target: None,
                flavor: None,
            },
        );
        assert!(result.is_err());
    }
}
