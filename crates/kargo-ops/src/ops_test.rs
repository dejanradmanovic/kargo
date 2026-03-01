//! Operation: build and run tests.
//!
//! Compiles main sources, then compiles test sources against main + test
//! dependencies, and runs the tests using JUnit console launcher or
//! kotlin-test runner.

use std::path::{Path, PathBuf};

use kargo_compiler::build_cache::BuildCache;
use kargo_compiler::classpath;
use kargo_compiler::dispatch::CompilerDispatch;
use kargo_compiler::env::BuildEnv;
use kargo_compiler::fingerprint;
use kargo_compiler::incremental::{self, IncrementalDecision};
use kargo_compiler::source_set_discovery::collect_kotlin_files;
use kargo_compiler::unit::CompilationUnit;
use kargo_maven::cache::LocalCache;
use kargo_util::errors::KargoError;

use crate::ops_build::{self, BuildOptions};

pub const JUNIT_PLATFORM_GROUP: &str = "org.junit.platform";
pub const JUNIT_PLATFORM_STANDALONE: &str = "junit-platform-console-standalone";
pub const JUNIT_PLATFORM_VERSION: &str = "1.11.4";

/// Run project tests.
pub fn test(
    project_dir: &Path,
    target: Option<&str>,
    filter: Option<&str>,
    verbose: bool,
) -> miette::Result<()> {
    use kargo_util::progress::status;

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
            message: "Build failed, cannot run tests.".into(),
        }
        .into());
    }

    // Reuse manifest, lockfile, and preflight from the build result
    let manifest = &build_result.manifest;
    let lockfile = &build_result.lockfile;
    let preflight = &build_result.preflight;

    let discovered =
        kargo_compiler::source_set_discovery::discover(project_dir, manifest);
    let mut test_kotlin_dirs: Vec<PathBuf> = Vec::new();
    for ss in &discovered.test_sources {
        test_kotlin_dirs.extend(ss.kotlin_dirs.clone());
    }
    let test_sources = collect_kotlin_files(&test_kotlin_dirs);

    if test_sources.is_empty() {
        status("Testing", "no test sources found");
        return Ok(());
    }

    status(
        "Testing",
        &format!("{} v{}", manifest.package.name, manifest.package.version),
    );

    let config = match kargo_core::config::GlobalConfig::load() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to load global config, using defaults: {e}");
            kargo_core::config::GlobalConfig::default()
        }
    };

    let test_classes_dir = build_result.build_dir.join("test-classes");
    std::fs::create_dir_all(&test_classes_dir).map_err(KargoError::Io)?;

    let cp = classpath::assemble(project_dir, lockfile);
    let mut test_classpath = vec![build_result.classes_dir.clone()];

    let gen_base = build_result.build_dir.join("generated");
    for subdir in &["ksp/classes", "kapt/classes"] {
        let dir = gen_base.join(subdir);
        if dir.is_dir() {
            test_classpath.push(dir);
        }
    }

    test_classpath.extend(cp.test_jars.clone());

    let kotlin_lib = preflight.toolchain.home.join("lib");
    for jar_name in kargo_compiler::classpath::STDLIB_RUNTIME_JARS
        .iter()
        .chain(&["kotlin-test.jar", "kotlin-test-junit5.jar", "kotlin-test-junit.jar"])
    {
        let jar = kotlin_lib.join(jar_name);
        if jar.is_file()
            && !test_classpath
                .iter()
                .any(|p| p.file_name() == jar.file_name())
        {
            test_classpath.push(jar);
        }
    }

    let junit_standalone = ensure_junit_platform(project_dir, lockfile)?;
    if let Some(ref jar) = junit_standalone {
        test_classpath.push(jar.clone());
    }

    let profile = manifest
        .profile
        .get(&build_result.profile_name)
        .cloned()
        .unwrap_or_else(kargo_core::profile::Profile::dev);

    let mut test_compiler_args = profile.compiler_args.clone();
    crate::ops_build::detect_compiler_plugins(
        lockfile,
        &preflight.toolchain.home,
        &mut test_compiler_args,
    );

    let test_unit = CompilationUnit {
        name: "test".into(),
        target: build_result.target,
        sources: test_sources,
        resource_dirs: discovered
            .test_sources
            .iter()
            .flat_map(|ss| ss.resource_dirs.clone())
            .collect(),
        classpath: test_classpath.clone(),
        output_dir: test_classes_dir.clone(),
        compiler_args: test_compiler_args,
        is_test: true,
        generated_sources: vec![],
        processor_jars: vec![],
    };
    let kotlin_ver = preflight.toolchain.version.to_string();
    let env = BuildEnv::new(
        manifest,
        project_dir,
        &build_result.build_dir,
        build_result.target.kebab_name(),
        &build_result.profile_name,
        &kotlin_ver,
        &preflight.toolchain.home,
        config.build.jobs,
    );

    let fp_dir = fingerprint::storage_dir(
        project_dir,
        build_result.target.kebab_name(),
        &build_result.profile_name,
    );
    let decision = incremental::check(&test_unit, &fp_dir, &kotlin_ver);

    match decision {
        IncrementalDecision::UpToDate => {
            if verbose {
                println!("  test: up-to-date (skipped)");
            }
        }
        IncrementalDecision::NeedsRebuild(fp) => {
            let build_cache = BuildCache::new(BuildCache::default_path(), None);
            if build_cache.restore(&fp, &test_classes_dir)? {
                if verbose {
                    println!("  test: restored from cache");
                }
                incremental::mark_complete(&fp_dir, "test", &fp, &test_unit)?;
            } else {
                let compiler = CompilerDispatch::resolve(
                    build_result.target,
                    preflight.toolchain.clone(),
                    preflight.jdk.home.clone(),
                    preflight.java_target.clone(),
                );

                let compile_output = compiler.compile(&test_unit, &env)?;
                if !compile_output.success {
                    for d in &compile_output.diagnostics {
                        eprintln!(
                            "{}: {}",
                            match d.severity {
                                kargo_compiler::unit::DiagnosticSeverity::Error => "error",
                                kargo_compiler::unit::DiagnosticSeverity::Warning => "warning",
                                kargo_compiler::unit::DiagnosticSeverity::Info => "info",
                            },
                            d.message
                        );
                    }
                    return Err(KargoError::Generic {
                        message: "Test compilation failed.".into(),
                    }
                    .into());
                }

                incremental::mark_complete(&fp_dir, "test", &fp, &test_unit)?;
                let _ = build_cache.put(&fp, &test_classes_dir);
            }
        }
    }

    // 5. Run tests using java
    status("Running", &format!("{} test(s)", test_unit.sources.len()));
    let java_bin = preflight.jdk.home.join("bin").join("java");

    let mut run_cp = vec![
        test_classes_dir.to_string_lossy().to_string(),
        build_result.classes_dir.to_string_lossy().to_string(),
    ];

    let resources_dir = build_result.build_dir.join("resources");
    if resources_dir.is_dir() {
        run_cp.push(resources_dir.to_string_lossy().to_string());
    }

    for jar_name in kargo_compiler::classpath::STDLIB_RUNTIME_JARS
        .iter()
        .chain(&["kotlin-test.jar", "kotlin-test-junit5.jar", "kotlin-test-junit.jar"])
    {
        let jar = kotlin_lib.join(jar_name);
        if jar.is_file() {
            run_cp.push(jar.to_string_lossy().to_string());
        }
    }

    run_cp.push(classpath::to_classpath_string(&cp.test_jars));

    let classpath_str = run_cp.join(if cfg!(windows) { ";" } else { ":" });

    let junit_jar = cp
        .test_jars
        .iter()
        .find(|p| {
            p.file_name()
                .map(|f| {
                    f.to_string_lossy()
                        .contains("junit-platform-console-standalone")
                })
                .unwrap_or(false)
        })
        .cloned()
        .or(junit_standalone);

    let output = if let Some(junit) = junit_jar {
        let mut cmd =
            kargo_util::process::CommandBuilder::new(java_bin.to_string_lossy().to_string())
                .arg("-jar")
                .arg(junit.to_string_lossy().to_string())
                .arg("execute")
                .arg("--class-path")
                .arg(&classpath_str)
                .arg("--scan-class-path");

        if let Some(f) = filter {
            cmd = cmd.arg("--include-classname").arg(f);
        }

        cmd = cmd.env(
            "JAVA_HOME",
            preflight.jdk.home.to_string_lossy().to_string(),
        );
        cmd.exec().map_err(|e| KargoError::Generic {
            message: format!("Failed to execute JUnit: {e}"),
        })?
    } else {
        let test_main_classes = detect_test_main_classes(&test_unit.sources, project_dir);

        if test_main_classes.is_empty() {
            return Err(KargoError::Generic {
                message: "No test main classes found. Add `fun main()` to test files \
                          or use JUnit with kotlin-test-junit/junit5."
                    .into(),
            }
            .into());
        }

        let mut last_output = None;
        for main_class in &test_main_classes {
            if let Some(ref f) = filter {
                if !main_class.contains(f) {
                    continue;
                }
            }

            let cmd =
                kargo_util::process::CommandBuilder::new(java_bin.to_string_lossy().to_string())
                    .arg("-cp")
                    .arg(&classpath_str)
                    .arg(main_class)
                    .env(
                        "JAVA_HOME",
                        preflight.jdk.home.to_string_lossy().to_string(),
                    );

            let result = cmd.exec().map_err(|e| KargoError::Generic {
                message: format!("Failed to execute test {main_class}: {e}"),
            })?;

            last_output = Some(result);
        }

        last_output.ok_or_else(|| KargoError::Generic {
            message: "No test classes matched the filter.".into(),
        })?
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stdout.is_empty() {
        print!("{stdout}");
    }
    if !stderr.is_empty() {
        eprint!("{stderr}");
    }

    if output.status.success() {
        status("Finished", "test result: ok");
        Ok(())
    } else {
        let code = output.status.code().unwrap_or(1);
        Err(KargoError::Generic {
            message: format!("Tests failed (exit code {code})"),
        }
        .into())
    }
}

fn detect_test_main_classes(test_sources: &[PathBuf], project_dir: &Path) -> Vec<String> {
    let mut classes = Vec::new();

    for file in test_sources {
        if let Ok(content) = std::fs::read_to_string(file) {
            if content.contains("fun main(") || content.contains("fun main()") {
                if let Some(class_name) = derive_test_class_name(file, &content, project_dir) {
                    classes.push(class_name);
                }
            }
        }
    }

    classes
}

fn ensure_junit_platform(
    project_dir: &Path,
    lockfile: &kargo_core::lockfile::Lockfile,
) -> miette::Result<Option<PathBuf>> {
    let needs_junit = lockfile.package.iter().any(|pkg| {
        pkg.name.starts_with("kotlin-test")
            || pkg.name.contains("junit")
            || pkg.name.contains("jupiter")
    });

    if !needs_junit {
        return Ok(None);
    }

    let cache = LocalCache::new(project_dir);

    if let Some(path) = cache.get_jar(
        JUNIT_PLATFORM_GROUP,
        JUNIT_PLATFORM_STANDALONE,
        JUNIT_PLATFORM_VERSION,
        None,
    ) {
        return Ok(Some(path));
    }

    let rt = tokio::runtime::Runtime::new().map_err(|e| KargoError::Generic {
        message: format!("Failed to create async runtime: {e}"),
    })?;

    let path = rt.block_on(kargo_compiler::plugins::ensure_maven_jar(
        &cache,
        JUNIT_PLATFORM_GROUP,
        JUNIT_PLATFORM_STANDALONE,
        JUNIT_PLATFORM_VERSION,
    ))?;

    Ok(path)
}

fn derive_test_class_name(file: &Path, content: &str, project_dir: &Path) -> Option<String> {
    let stem = file.file_stem()?.to_string_lossy();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("package ") {
            let pkg = trimmed
                .trim_start_matches("package ")
                .trim_end_matches(';')
                .trim();
            return Some(format!("{pkg}.{stem}Kt"));
        }
    }

    let test_roots = [
        project_dir.join("src/test/kotlin"),
        project_dir.join("src/commonTest/kotlin"),
        project_dir.join("src/jvmTest/kotlin"),
    ];

    for root in &test_roots {
        if let Ok(rel) = file.strip_prefix(root) {
            let class = rel
                .with_extension("")
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, ".");
            return Some(format!("{class}Kt"));
        }
    }

    Some(format!("{stem}Kt"))
}
