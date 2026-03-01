//! Operation: type-check without producing output artifacts.
//!
//! Runs the compiler in check-only mode to validate sources and report
//! diagnostics without writing .class files to the build directory.

use std::path::Path;

use kargo_compiler::dispatch::CompilerDispatch;
use kargo_compiler::source_set_discovery::collect_kotlin_files;
use kargo_compiler::unit::CompilationUnit;
use kargo_util::errors::KargoError;

use crate::ops_setup;

/// Type-check the project without producing output artifacts.
pub fn check(project_dir: &Path, verbose: bool) -> miette::Result<()> {
    let ctx = crate::BuildContext::load(project_dir, None, None, false)?;

    if verbose {
        ops_setup::print_preflight_summary(&ctx.preflight);
        println!();
    }

    kargo_util::progress::status(
        "Checking",
        &format!("{} v{}", ctx.manifest.package.name, ctx.manifest.package.version),
    );

    let mut all_kotlin_dirs: Vec<std::path::PathBuf> = Vec::new();
    for ss in &ctx.discovered.main_sources {
        all_kotlin_dirs.extend(ss.kotlin_dirs.clone());
    }
    let main_sources = collect_kotlin_files(&all_kotlin_dirs);

    if main_sources.is_empty() {
        println!("No Kotlin source files found to check.");
        return Ok(());
    }

    let unit = CompilationUnit {
        name: "check".into(),
        target: ctx.target,
        sources: main_sources,
        resource_dirs: vec![],
        classpath: ctx.classpath.compile_jars,
        output_dir: ctx.build_dir.join("check-output"),
        compiler_args: ctx.profile.compiler_args.clone(),
        is_test: false,
        generated_sources: vec![],
        processor_jars: vec![],
    };

    let compiler = CompilerDispatch::resolve(
        ctx.target,
        ctx.preflight.toolchain.clone(),
        ctx.preflight.jdk.home.clone(),
        ctx.preflight.java_target.clone(),
    );

    let output = compiler.check_only(&unit, &ctx.env)?;

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
