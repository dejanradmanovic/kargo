//! Operation: type-check without producing output artifacts.
//!
//! Runs the compiler in check-only mode to validate sources and report
//! diagnostics without writing .class files to the build directory.

use std::path::Path;

use kargo_compiler::classpath;
use kargo_compiler::dispatch::CompilerDispatch;
use kargo_compiler::env::BuildEnv;
use kargo_compiler::source_set_discovery::{self, collect_kotlin_files};
use kargo_compiler::unit::CompilationUnit;
use kargo_core::lockfile::Lockfile;
use kargo_core::manifest::Manifest;
use kargo_core::target::KotlinTarget;
use kargo_util::errors::KargoError;

use crate::ops_setup;

/// Type-check the project without producing output artifacts.
pub fn check(project_dir: &Path, verbose: bool) -> miette::Result<()> {
    let preflight = ops_setup::preflight(project_dir)?;
    if verbose {
        ops_setup::print_preflight_summary(&preflight);
        println!();
    }

    ops_setup::ensure_lockfile(project_dir)?;

    let manifest = Manifest::from_path(&project_dir.join("Kargo.toml"))?;
    kargo_util::progress::status(
        "Checking",
        &format!("{} v{}", manifest.package.name, manifest.package.version),
    );
    let lockfile = Lockfile::from_path(&project_dir.join("Kargo.lock"))
        .unwrap_or(Lockfile { package: vec![] });

    let target_name = manifest
        .targets
        .keys()
        .next()
        .map(|s| s.as_str())
        .unwrap_or("jvm");

    let target = KotlinTarget::parse(target_name).ok_or_else(|| KargoError::Generic {
        message: format!("Unknown target '{target_name}'"),
    })?;

    let discovered = source_set_discovery::discover(project_dir, &manifest);
    let mut all_kotlin_dirs: Vec<std::path::PathBuf> = Vec::new();
    for ss in &discovered.main_sources {
        all_kotlin_dirs.extend(ss.kotlin_dirs.clone());
    }
    let main_sources = collect_kotlin_files(&all_kotlin_dirs);

    if main_sources.is_empty() {
        println!("No Kotlin source files found to check.");
        return Ok(());
    }

    let cp = classpath::assemble(project_dir, &lockfile);
    let profile = manifest
        .profile
        .get("dev")
        .cloned()
        .unwrap_or_else(kargo_core::profile::Profile::dev);

    let build_dir = project_dir
        .join("build")
        .join(target.kebab_name())
        .join("dev");
    let config = kargo_core::config::GlobalConfig::load().unwrap_or_default();

    let kotlin_ver = preflight.toolchain.version.to_string();
    let env = BuildEnv::new(
        &manifest,
        project_dir,
        &build_dir,
        target.kebab_name(),
        "dev",
        &kotlin_ver,
        &preflight.toolchain.home,
        config.build.jobs,
    );

    let unit = CompilationUnit {
        name: "check".into(),
        target,
        sources: main_sources,
        resource_dirs: vec![],
        classpath: cp.compile_jars,
        output_dir: build_dir.join("check-output"),
        compiler_args: profile.compiler_args.clone(),
        is_test: false,
        generated_sources: vec![],
        processor_jars: vec![],
    };

    let compiler = CompilerDispatch::resolve(
        target,
        preflight.toolchain.clone(),
        preflight.jdk.home.clone(),
        preflight.java_target.clone(),
    );

    let output = compiler.check_only(&unit, &env)?;

    for d in &output.diagnostics {
        let prefix = match d.severity {
            kargo_compiler::unit::DiagnosticSeverity::Error => "error",
            kargo_compiler::unit::DiagnosticSeverity::Warning => "warning",
            kargo_compiler::unit::DiagnosticSeverity::Info => "info",
        };
        let location = match (&d.file, d.line) {
            (Some(f), Some(l)) => format!("{f}:{l}: "),
            (Some(f), None) => format!("{f}: "),
            _ => String::new(),
        };
        eprintln!("{location}{prefix}: {}", d.message);
    }

    if output.success {
        kargo_util::progress::status("Finished", "check passed");
        Ok(())
    } else {
        Err(KargoError::Generic {
            message: "Type-check failed.".into(),
        }
        .into())
    }
}
