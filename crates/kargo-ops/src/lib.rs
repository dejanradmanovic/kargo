pub mod ops_add;
pub mod ops_audit;
pub mod ops_build;
pub mod ops_cache;
pub mod ops_check;
pub mod ops_clean;
pub mod ops_fetch;
pub mod ops_init;
pub mod ops_lock;
pub mod ops_new;
pub mod ops_outdated;
pub mod ops_remove;
pub mod ops_run;
pub mod ops_self;
pub mod ops_self_update;
pub mod ops_setup;
pub mod ops_test;
pub mod ops_toolchain;
pub mod ops_tree;
pub mod ops_update;

use std::path::{Path, PathBuf};

use kargo_compiler::classpath::{self, Classpath};
use kargo_compiler::env::BuildEnv;
use kargo_compiler::source_set_discovery::{self, DiscoveredSources};
use kargo_core::config::GlobalConfig;
use kargo_core::lockfile::Lockfile;
use kargo_core::manifest::Manifest;
use kargo_core::target::KotlinTarget;
use kargo_util::errors::KargoError;

use crate::ops_setup::PreflightResult;

/// Shared build context assembled once and reused by build, test, run, check.
pub struct BuildContext {
    pub project_dir: PathBuf,
    pub manifest: Manifest,
    pub lockfile: Lockfile,
    pub preflight: PreflightResult,
    pub config: GlobalConfig,
    pub target: KotlinTarget,
    pub profile: kargo_core::profile::Profile,
    pub profile_name: String,
    pub build_dir: PathBuf,
    pub classes_dir: PathBuf,
    pub resources_dir: PathBuf,
    pub generated_dir: PathBuf,
    pub classpath: Classpath,
    pub env: BuildEnv,
    pub discovered: DiscoveredSources,
}

impl BuildContext {
    /// Load all project metadata and resolve build configuration.
    pub fn load(
        project_dir: &Path,
        target: Option<&str>,
        profile: Option<&str>,
        release: bool,
    ) -> miette::Result<Self> {
        let preflight = crate::ops_setup::preflight(project_dir)?;
        crate::ops_setup::ensure_lockfile(project_dir)?;

        let manifest = Manifest::from_path(&project_dir.join("Kargo.toml"))?;
        let lockfile = Lockfile::from_path(&project_dir.join("Kargo.lock"))
            .unwrap_or(Lockfile { package: vec![] });

        let target_name = target
            .or_else(|| manifest.targets.keys().next().map(|s| s.as_str()))
            .unwrap_or("jvm");

        let kotlin_target = KotlinTarget::parse(target_name).ok_or_else(|| KargoError::Generic {
            message: format!(
                "Unknown target '{}'. Available: {}",
                target_name,
                manifest.targets.keys().cloned().collect::<Vec<_>>().join(", ")
            ),
        })?;

        let profile_name = if let Some(p) = profile {
            p.to_string()
        } else if release {
            "release".to_string()
        } else {
            "dev".to_string()
        };

        let resolved_profile = manifest
            .profile
            .get(&profile_name)
            .cloned()
            .unwrap_or_else(|| {
                if profile_name == "release" {
                    kargo_core::profile::Profile::release()
                } else {
                    kargo_core::profile::Profile::dev()
                }
            });

        let build_dir = project_dir
            .join("build")
            .join(kotlin_target.kebab_name())
            .join(&profile_name);
        std::fs::create_dir_all(&build_dir).map_err(KargoError::Io)?;

        let classes_dir = build_dir.join("classes");
        let resources_dir = build_dir.join("resources");
        let generated_dir = build_dir.join("generated");
        std::fs::create_dir_all(&classes_dir).map_err(KargoError::Io)?;
        std::fs::create_dir_all(&resources_dir).map_err(KargoError::Io)?;
        std::fs::create_dir_all(&generated_dir).map_err(KargoError::Io)?;

        let config = match GlobalConfig::load() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to load global config, using defaults: {e}");
                GlobalConfig::default()
            }
        };

        let kotlin_ver = preflight.toolchain.version.to_string();
        let env = BuildEnv::new(
            &manifest,
            project_dir,
            &build_dir,
            kotlin_target.kebab_name(),
            &profile_name,
            &kotlin_ver,
            &preflight.toolchain.home,
            config.build.jobs,
        );

        let cp = classpath::assemble(project_dir, &lockfile);
        let discovered = source_set_discovery::discover(project_dir, &manifest);

        Ok(BuildContext {
            project_dir: project_dir.to_path_buf(),
            manifest,
            lockfile,
            preflight,
            config,
            target: kotlin_target,
            profile: resolved_profile,
            profile_name,
            build_dir,
            classes_dir,
            resources_dir,
            generated_dir,
            classpath: cp,
            env,
            discovered,
        })
    }
}

/// Re-export `classpath_string_with_stdlib` from the compiler crate for convenience.
pub fn classpath_string_with_stdlib(jars: &[PathBuf], kotlin_home: &Path) -> String {
    kargo_compiler::classpath::classpath_string_with_stdlib(jars, kotlin_home)
}
