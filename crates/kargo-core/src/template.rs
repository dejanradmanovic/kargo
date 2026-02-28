//! Embedded project template system for `kargo new` / `kargo init`.
//!
//! Templates are TOML descriptors compiled into the binary via `include_str!`.
//! Each template declares the directories, files, and `Kargo.toml` content to
//! generate for a new project. Simple `{{variable}}` interpolation is performed
//! at render time.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

use kargo_util::errors::KargoError;

/// Metadata about a project template (name, description).
#[derive(Debug, Clone, Deserialize)]
pub struct TemplateMeta {
    pub name: String,
    pub description: String,
}

/// The manifest section — raw `Kargo.toml` content with `{{variable}}` placeholders.
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestTemplate {
    pub content: String,
}

/// A directory to create during project scaffolding.
#[derive(Debug, Clone, Deserialize)]
pub struct DirectoryEntry {
    pub path: String,
}

/// A file to create during project scaffolding, with interpolated content.
#[derive(Debug, Clone, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub content: String,
}

/// A complete project template parsed from a TOML descriptor.
#[derive(Debug, Clone, Deserialize)]
pub struct ProjectTemplate {
    pub template: TemplateMeta,
    pub manifest: ManifestTemplate,
    #[serde(default)]
    pub directories: Vec<DirectoryEntry>,
    #[serde(default)]
    pub files: Vec<FileEntry>,
}

/// Variables available for `{{variable}}` interpolation in template content.
pub struct TemplateContext {
    vars: BTreeMap<String, String>,
}

impl TemplateContext {
    /// Create a context with the standard project variables.
    pub fn new(project_name: &str, kotlin_version: &str) -> Self {
        let mut vars = BTreeMap::new();
        vars.insert("project_name".to_string(), project_name.to_string());
        vars.insert("kotlin_version".to_string(), kotlin_version.to_string());
        Self { vars }
    }

    /// Add a custom variable to the context.
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.vars.insert(key.into(), value.into());
    }
}

/// Replace all `{{key}}` placeholders in `input` with values from `ctx`.
/// Unknown keys are replaced with an empty string.
pub fn interpolate(input: &str, ctx: &TemplateContext) -> String {
    let mut result = input.to_string();
    for (key, value) in &ctx.vars {
        let placeholder = format!("{{{{{}}}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

impl ProjectTemplate {
    /// Parse a template from a TOML string.
    pub fn parse_toml(toml_str: &str) -> miette::Result<Self> {
        toml::from_str(toml_str).map_err(|e| {
            KargoError::Generic {
                message: format!("Failed to parse project template: {e}"),
            }
            .into()
        })
    }

    /// Render the full template (directories, source files, and core files)
    /// into a directory. Used by `kargo new`.
    pub fn render(&self, root: &Path, ctx: &TemplateContext) -> miette::Result<()> {
        for dir in &self.directories {
            let path = root.join(&dir.path);
            std::fs::create_dir_all(&path).map_err(KargoError::Io)?;
        }

        self.write_core_files(root, ctx, false)?;

        for file in &self.files {
            let path = root.join(&file.path);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(KargoError::Io)?;
            }
            let content = interpolate(&file.content, ctx);
            std::fs::write(&path, content).map_err(KargoError::Io)?;
        }

        Ok(())
    }

    /// Render only the core project files (`Kargo.toml`, `Kargo.lock`,
    /// `.kargo.env`, `.gitignore`) plus any template-defined non-source files,
    /// without creating source directories or sample source files. Used by
    /// `kargo init` on an existing project.
    ///
    /// Existing files are never overwritten.
    pub fn render_core_only(&self, root: &Path, ctx: &TemplateContext) -> miette::Result<()> {
        self.write_core_files(root, ctx, true)?;
        self.write_non_source_files(root, ctx, true)
    }

    fn write_core_files(
        &self,
        root: &Path,
        ctx: &TemplateContext,
        skip_existing: bool,
    ) -> miette::Result<()> {
        let write = |path: std::path::PathBuf, content: &str| -> miette::Result<()> {
            if skip_existing && path.exists() {
                return Ok(());
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(KargoError::Io)?;
            }
            std::fs::write(&path, content).map_err(KargoError::Io)?;
            Ok(())
        };

        write(
            root.join("Kargo.toml"),
            &interpolate(&self.manifest.content, ctx),
        )?;

        write(
            root.join("Kargo.lock"),
            "# This file is auto-generated by Kargo. Commit it for applications.\n",
        )?;

        write(root.join(".gitignore"), "build/\n.kargo/\n.kargo.env\n")?;

        write(
            root.join(".kargo.env"),
            "# Build secrets and credentials (this file is gitignored)\n\
             # Values here are available via ${env:VAR} in Kargo.toml\n\
             # and as regular env vars during builds, hooks, and plugins.\n",
        )?;

        Ok(())
    }

    /// Write template `[[files]]` entries whose paths do not start with `src/`.
    fn write_non_source_files(
        &self,
        root: &Path,
        ctx: &TemplateContext,
        skip_existing: bool,
    ) -> miette::Result<()> {
        for file in &self.files {
            if file.path.starts_with("src/") {
                continue;
            }
            let path = root.join(&file.path);
            if skip_existing && path.exists() {
                continue;
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(KargoError::Io)?;
            }
            let content = interpolate(&file.content, ctx);
            std::fs::write(&path, content).map_err(KargoError::Io)?;
        }
        Ok(())
    }
}

/// Registry of all built-in project templates.
pub struct TemplateRegistry {
    templates: BTreeMap<String, ProjectTemplate>,
}

impl TemplateRegistry {
    /// Build the registry from all embedded template TOML files.
    pub fn new() -> miette::Result<Self> {
        let raw_templates: Vec<(&str, &str)> = vec![
            ("jvm", include_str!("../templates/jvm.toml")),
            ("lib", include_str!("../templates/lib.toml")),
            ("kmp", include_str!("../templates/kmp.toml")),
            ("cmp", include_str!("../templates/cmp.toml")),
            ("android", include_str!("../templates/android.toml")),
        ];

        let mut templates = BTreeMap::new();
        for (name, src) in raw_templates {
            let tmpl = ProjectTemplate::parse_toml(src).map_err(|_| KargoError::Generic {
                message: format!("Built-in template '{name}' is malformed"),
            })?;
            templates.insert(name.to_string(), tmpl);
        }

        Ok(Self { templates })
    }

    /// Look up a template by name.
    pub fn get(&self, name: &str) -> Option<&ProjectTemplate> {
        self.templates.get(name)
    }

    /// List all available template names with descriptions.
    pub fn list(&self) -> Vec<(&str, &str)> {
        self.templates
            .iter()
            .map(|(k, v)| (k.as_str(), v.template.description.as_str()))
            .collect()
    }

    /// Return all valid template names (for CLI validation).
    pub fn names(&self) -> Vec<&str> {
        self.templates.keys().map(|k| k.as_str()).collect()
    }
}
