//! Operation: remove a dependency from Kargo.toml.

use std::path::Path;

use toml_edit::DocumentMut;

/// Options for `kargo remove`.
pub struct RemoveOptions {
    /// The dependency name (artifact ID key in the TOML table).
    pub name: String,
    /// Remove from dev dependencies.
    pub dev: bool,
    /// Remove from a specific target section.
    pub target: Option<String>,
    /// Remove from a specific flavor section.
    pub flavor: Option<String>,
}

/// Remove a dependency from `Kargo.toml` using format-preserving edits.
pub fn remove_dependency(manifest_path: &Path, opts: &RemoveOptions) -> miette::Result<()> {
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

    let removed = if let Some(ref target) = opts.target {
        remove_key_at(&mut doc, &["target", target, "dependencies", &opts.name])
    } else if let Some(ref flavor) = opts.flavor {
        remove_key_at(&mut doc, &["flavor", flavor, "dependencies", &opts.name])
    } else if opts.dev {
        remove_key_at(&mut doc, &["dev-dependencies", &opts.name])
    } else {
        remove_key_at(&mut doc, &["dependencies", &opts.name])
    };

    if !removed {
        return Err(kargo_util::errors::KargoError::Generic {
            message: format!("Dependency '{}' not found in Kargo.toml", opts.name),
        }
        .into());
    }

    std::fs::write(manifest_path, doc.to_string())
        .map_err(|e| kargo_util::errors::KargoError::Io(e).into())
}

/// Navigate a TOML document path and remove the leaf key.
fn remove_key_at(doc: &mut DocumentMut, path: &[&str]) -> bool {
    if path.is_empty() {
        return false;
    }
    if path.len() == 1 {
        return doc.remove(path[0]).is_some();
    }

    let mut current = doc.as_table_mut();
    for &key in &path[..path.len() - 1] {
        match current.get_mut(key) {
            Some(toml_edit::Item::Table(ref mut t)) => current = t,
            _ => return false,
        }
    }
    current.remove(path[path.len() - 1]).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_existing_dependency() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Kargo.toml");
        std::fs::write(
            &path,
            r#"[package]
name = "test"
version = "0.1.0"
kotlin = "2.3.0"

[dependencies]
coroutines = "org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0"
ktor = "io.ktor:ktor-core:2.3.0"
"#,
        )
        .unwrap();

        remove_dependency(
            &path,
            &RemoveOptions {
                name: "coroutines".to_string(),
                dev: false,
                target: None,
                flavor: None,
            },
        )
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(!content.contains("coroutines"));
        assert!(content.contains("ktor"));
    }

    #[test]
    fn remove_nonexistent_dependency() {
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

        let result = remove_dependency(
            &path,
            &RemoveOptions {
                name: "missing".to_string(),
                dev: false,
                target: None,
                flavor: None,
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn preserves_formatting() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("Kargo.toml");
        let original = r#"[package]
name = "test"
version = "0.1.0"
kotlin = "2.3.0"

# Main dependencies
[dependencies]
coroutines = "org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0"
ktor = "io.ktor:ktor-core:2.3.0"
"#;
        std::fs::write(&path, original).unwrap();

        remove_dependency(
            &path,
            &RemoveOptions {
                name: "coroutines".to_string(),
                dev: false,
                target: None,
                flavor: None,
            },
        )
        .unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // Comment and other entries preserved
        assert!(content.contains("# Main dependencies"));
        assert!(content.contains("ktor"));
    }
}
