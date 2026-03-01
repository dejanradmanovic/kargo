//! Build environment variables set during compilation and hook execution.
//!
//! Provides standardized `KARGO_*` environment variables as defined in
//! Section 12 of the architecture document.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use kargo_core::manifest::Manifest;

/// Collected build environment variables.
#[derive(Debug, Clone)]
pub struct BuildEnv {
    pub vars: HashMap<String, String>,
}

impl BuildEnv {
    /// Build the environment variable set for a compilation.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        manifest: &Manifest,
        project_root: &Path,
        build_dir: &Path,
        target_name: &str,
        profile: &str,
        kotlin_version: &str,
        toolchain_dir: &Path,
        jobs: u32,
    ) -> Self {
        let mut vars = HashMap::new();

        // Package variables
        vars.insert(
            "KARGO_MANIFEST_DIR".into(),
            project_root.to_string_lossy().into(),
        );
        vars.insert("KARGO_PKG_NAME".into(), manifest.package.name.clone());
        vars.insert("KARGO_PKG_VERSION".into(), manifest.package.version.clone());

        if let Some((major, rest)) = manifest.package.version.split_once('.') {
            vars.insert("KARGO_PKG_VERSION_MAJOR".into(), major.into());
            if let Some((minor, patch)) = rest.split_once('.') {
                vars.insert("KARGO_PKG_VERSION_MINOR".into(), minor.into());
                vars.insert("KARGO_PKG_VERSION_PATCH".into(), patch.into());
            }
        }

        if let Some(desc) = &manifest.package.description {
            vars.insert("KARGO_PKG_DESCRIPTION".into(), desc.clone());
        }
        if !manifest.package.authors.is_empty() {
            vars.insert(
                "KARGO_PKG_AUTHORS".into(),
                manifest.package.authors.join(", "),
            );
        }
        if let Some(repo) = &manifest.package.repository {
            vars.insert("KARGO_PKG_REPOSITORY".into(), repo.clone());
        }

        // Build context variables
        vars.insert("KARGO_BUILD_DIR".into(), build_dir.to_string_lossy().into());
        vars.insert("KARGO_TARGET".into(), target_name.into());
        vars.insert("KARGO_PROFILE".into(), profile.into());
        vars.insert("KARGO_JOBS".into(), jobs.to_string());
        vars.insert("KARGO_KOTLIN_VERSION".into(), kotlin_version.into());
        vars.insert(
            "KARGO_TOOLCHAIN_DIR".into(),
            toolchain_dir.to_string_lossy().into(),
        );

        Self { vars }
    }

    /// Set flavor/variant variables if flavors are configured.
    pub fn set_variant(&mut self, variant_name: &str, flavor_values: &[(String, String)]) {
        self.vars
            .insert("KARGO_VARIANT".into(), variant_name.into());
        for (dimension, value) in flavor_values {
            let key = format!("KARGO_FLAVOR_{}", dimension.to_uppercase());
            self.vars.insert(key, value.clone());
        }
    }

    /// Set build-config entries as `KARGO_BUILD_CONFIG_*` environment variables.
    pub fn set_build_config(&mut self, entries: &[(String, String)]) {
        for (key, value) in entries {
            let env_key = format!("KARGO_BUILD_CONFIG_{key}");
            self.vars.insert(env_key, value.clone());
        }
    }

    /// Return the dependencies directory path.
    pub fn cache_dir(project_root: &Path) -> PathBuf {
        project_root.join(".kargo").join("dependencies")
    }
}
