//! Operation: build and run the project.
//!
//! Compiles main sources via `ops_build`, then invokes `java` with the
//! compiled classpath to run the application.

use std::path::Path;

use kargo_compiler::classpath;
use kargo_core::lockfile::Lockfile;
use kargo_core::manifest::Manifest;
use kargo_util::errors::KargoError;

use crate::ops_build::{self, BuildOptions};

/// Run the project after building.
pub fn run(
    project_dir: &Path,
    target: Option<&str>,
    run_args: &[String],
    verbose: bool,
) -> miette::Result<()> {
    let build_result = ops_build::build(
        project_dir,
        &BuildOptions {
            target: target.map(String::from),
            verbose,
            quiet: true,
            ..Default::default()
        },
    )?;

    if !build_result.success {
        return Err(KargoError::Generic {
            message: "Build failed, cannot run.".into(),
        }
        .into());
    }

    let manifest = Manifest::from_path(&project_dir.join("Kargo.toml"))?;
    let lockfile = Lockfile::from_path(&project_dir.join("Kargo.lock"))
        .unwrap_or(Lockfile { package: vec![] });

    // Detect main class
    let main_class = manifest
        .package
        .main_class
        .clone()
        .or_else(|| detect_main_class(project_dir))
        .ok_or_else(|| KargoError::Generic {
            message: "Could not detect main class. Set [package] main-class in Kargo.toml \
                      or add a file containing `fun main()`."
                .into(),
        })?;

    let preflight = crate::ops_setup::preflight(project_dir)?;

    // Build classpath: compiled classes + Kotlin runtime + dependency JARs
    let cp = classpath::assemble(project_dir, &lockfile);
    let mut cp_parts = vec![build_result.classes_dir.to_string_lossy().to_string()];

    let resources_dir = build_result.build_dir.join("resources");
    if resources_dir.is_dir() {
        cp_parts.push(resources_dir.to_string_lossy().to_string());
    }

    // Kotlin stdlib from the toolchain (always needed at runtime)
    let kotlin_lib = preflight.toolchain.home.join("lib");
    for jar_name in &[
        "kotlin-stdlib.jar",
        "kotlin-stdlib-jdk8.jar",
        "kotlin-stdlib-jdk7.jar",
    ] {
        let jar = kotlin_lib.join(jar_name);
        if jar.is_file() {
            cp_parts.push(jar.to_string_lossy().to_string());
        }
    }

    if !cp.compile_jars.is_empty() {
        cp_parts.push(classpath::to_classpath_string(&cp.compile_jars));
    }

    let classpath_str = cp_parts.join(if cfg!(windows) { ";" } else { ":" });
    let java_bin = preflight.jdk.home.join("bin").join("java");

    kargo_util::progress::status("Running", &main_class);
    if verbose {
        eprintln!("  java: {}", java_bin.display());
    }

    let mut cmd = kargo_util::process::CommandBuilder::new(java_bin.to_string_lossy().to_string())
        .arg("-cp")
        .arg(&classpath_str)
        .arg(&main_class)
        .args(run_args.iter().cloned());

    cmd = cmd.env(
        "JAVA_HOME",
        preflight.jdk.home.to_string_lossy().to_string(),
    );

    let output = cmd.exec().map_err(|e| KargoError::Generic {
        message: format!("Failed to execute java: {e}"),
    })?;

    // Print stdout/stderr from the running program
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stdout.is_empty() {
        print!("{stdout}");
    }
    if !stderr.is_empty() {
        eprint!("{stderr}");
    }

    if !output.status.success() {
        let code = output.status.code().unwrap_or(1);
        return Err(KargoError::Generic {
            message: format!("Process exited with code {code}"),
        }
        .into());
    }

    Ok(())
}

/// Scan source files for a `fun main()` declaration and derive the class name.
fn detect_main_class(project_dir: &Path) -> Option<String> {
    let src_dirs = vec![
        project_dir.join("src/main/kotlin"),
        project_dir.join("src/commonMain/kotlin"),
        project_dir.join("src/jvmMain/kotlin"),
    ];

    let files = kargo_compiler::source_set_discovery::collect_kotlin_files(&src_dirs);

    for file in &files {
        if let Ok(content) = std::fs::read_to_string(file) {
            if content.contains("fun main(") || content.contains("fun main()") {
                return derive_main_class(file, project_dir);
            }
        }
    }

    None
}

/// Derive a JVM class name from a .kt file path.
///
/// Kotlin top-level `fun main()` in `com/example/Main.kt` becomes
/// `com.example.MainKt` on the JVM.
fn derive_main_class(file: &Path, project_dir: &Path) -> Option<String> {
    // Try to extract package from file content
    if let Ok(content) = std::fs::read_to_string(file) {
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("package ") {
                let pkg = trimmed
                    .trim_start_matches("package ")
                    .trim_end_matches(';')
                    .trim();
                let stem = file.file_stem()?.to_string_lossy();
                return Some(format!("{pkg}.{stem}Kt"));
            }
        }
    }

    // Fallback: derive from file path relative to source root
    let src_roots = [
        project_dir.join("src/main/kotlin"),
        project_dir.join("src/commonMain/kotlin"),
        project_dir.join("src/jvmMain/kotlin"),
    ];

    for root in &src_roots {
        if let Ok(rel) = file.strip_prefix(root) {
            let class = rel
                .with_extension("")
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, ".");
            return Some(format!("{class}Kt"));
        }
    }

    None
}
