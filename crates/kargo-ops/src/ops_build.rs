//! Operation: build the project (resolve, fetch, compile, link).
//!
//! Orchestrates the full build pipeline: preflight -> lockfile -> source discovery ->
//! classpath assembly -> KSP/KAPT -> compilation -> resource copy.

use std::path::{Path, PathBuf};
use std::time::Instant;

use kargo_compiler::build_cache::BuildCache;
use kargo_compiler::classpath;
use kargo_compiler::dispatch::CompilerDispatch;
use kargo_compiler::env::BuildEnv;
use kargo_compiler::fingerprint;
use kargo_compiler::incremental::{self, IncrementalDecision};
use kargo_compiler::plugins;
use kargo_compiler::source_set_discovery::{self, collect_kotlin_files};
use kargo_compiler::unit::CompilationUnit;
use kargo_compiler::unit_graph::UnitGraph;
use kargo_core::lockfile::Lockfile;
use kargo_core::manifest::Manifest;
use kargo_core::target::KotlinTarget;
use kargo_util::errors::KargoError;

use crate::ops_setup;

/// Options for a build invocation.
#[derive(Default)]
pub struct BuildOptions {
    pub target: Option<String>,
    pub profile: Option<String>,
    pub release: bool,
    pub verbose: bool,
    pub timings: bool,
    pub offline: bool,
    /// Suppress non-error output (used by `kargo run` / `kargo test`).
    pub quiet: bool,
}

/// Result of a build operation.
pub struct BuildResult {
    pub target: KotlinTarget,
    pub profile_name: String,
    pub build_dir: PathBuf,
    pub classes_dir: PathBuf,
    /// Path to the packaged output JAR, if produced.
    pub output_jar: Option<PathBuf>,
    pub success: bool,
}

/// Run the full build pipeline.
pub fn build(project_dir: &Path, opts: &BuildOptions) -> miette::Result<BuildResult> {
    let start = Instant::now();

    use kargo_util::progress::status;

    // 1. Preflight
    let preflight = ops_setup::preflight(project_dir)?;
    if opts.verbose {
        ops_setup::print_preflight_summary(&preflight);
        println!();
    }

    // 2. Ensure lockfile
    ops_setup::ensure_lockfile(project_dir)?;

    // 3. Load manifest and lockfile
    let manifest = Manifest::from_path(&project_dir.join("Kargo.toml"))?;
    let lockfile = Lockfile::from_path(&project_dir.join("Kargo.lock"))?;

    // 4. Determine target and profile
    let target = resolve_target(&manifest, opts.target.as_deref())?;
    let profile_name = resolve_profile(opts.profile.as_deref(), opts.release);

    if !opts.quiet {
        status(
            "Compiling",
            &format!(
                "{} v{} ({} {})",
                manifest.package.name, manifest.package.version, target, profile_name
            ),
        );
    }
    let profile = manifest
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

    let is_debug = profile.debug.unwrap_or(profile_name != "release");

    // 5. Build output directory
    let build_dir = project_dir
        .join("build")
        .join(target.kebab_name())
        .join(&profile_name);
    std::fs::create_dir_all(&build_dir).map_err(KargoError::Io)?;

    let classes_dir = build_dir.join("classes");
    let resources_dir = build_dir.join("resources");
    let generated_dir = build_dir.join("generated");
    std::fs::create_dir_all(&classes_dir).map_err(KargoError::Io)?;
    std::fs::create_dir_all(&resources_dir).map_err(KargoError::Io)?;
    std::fs::create_dir_all(&generated_dir).map_err(KargoError::Io)?;

    // 6. Build environment
    let config = kargo_core::config::GlobalConfig::load().unwrap_or_default();
    let kotlin_ver = preflight.toolchain.version.to_string();
    let env = BuildEnv::new(
        &manifest,
        project_dir,
        &build_dir,
        target.kebab_name(),
        &profile_name,
        &kotlin_ver,
        &preflight.toolchain.home,
        config.build.jobs,
    );

    // 7. Source discovery
    let discovered = source_set_discovery::discover(project_dir, &manifest);

    // 8. Classpath assembly
    let cp = classpath::assemble(project_dir, &lockfile);

    // 9. Collect main source files
    let mut all_kotlin_dirs: Vec<PathBuf> = Vec::new();
    for ss in &discovered.main_sources {
        all_kotlin_dirs.extend(ss.kotlin_dirs.clone());
    }
    let main_sources = collect_kotlin_files(&all_kotlin_dirs);

    if main_sources.is_empty() {
        println!("No Kotlin source files found to compile.");
        return Ok(BuildResult {
            target,
            profile_name,
            build_dir: build_dir.clone(),
            classes_dir,
            output_jar: None,
            success: true,
        });
    }

    // 10. Generate BuildConfig.kt
    // Prefer explicit `group` for the Kotlin package; fall back to deriving it from `main-class`.
    let kotlin_package = manifest
        .package
        .group
        .clone()
        .or_else(|| {
            manifest
                .package
                .main_class
                .as_deref()
                .and_then(kargo_compiler::buildconfig::package_from_main_class)
        });

    // Start with base [build-config], then merge flavor-specific values on top.
    // Flavor values override base values when keys collide.
    let mut build_config_fields = manifest.build_config.clone();
    if let Some(ref flavors) = manifest.flavors {
        // TODO: honour --flavor / --variant CLI flags once variant selection lands.
        // For now, use the declared default flavors (if any).
        let selected: std::collections::BTreeMap<String, String> = flavors
            .default
            .clone()
            .unwrap_or_default();

        for (dimension, flavor_name) in &selected {
            if let Some(dim_map) = flavors.dimension_flavors.get(dimension) {
                if let Some(def) = dim_map.get(flavor_name) {
                    for (k, v) in &def.build_config {
                        build_config_fields.insert(k.clone(), v.clone());
                    }
                }
            }
        }
    }

    let _buildconfig_path = kargo_compiler::buildconfig::generate(
        &generated_dir,
        kotlin_package.as_deref(),
        &manifest.package.name,
        &manifest.package.version,
        &profile_name,
        is_debug,
        &build_config_fields,
    )?;

    // 11. KSP/KAPT annotation processing — run as a pre-build step
    let cache = kargo_maven::cache::LocalCache::new(project_dir);
    let processors = plugins::detect_processors(&manifest, &cache);
    let generated_source_dirs: Vec<PathBuf> = vec![generated_dir.clone()];
    #[allow(unused_assignments)]
    let mut ksp_toolchain: Option<plugins::KspToolchain> = None;

    if !processors.is_empty() {
        // Quick mtime check: skip annotation processing entirely if sources,
        // processor JARs and Kargo.toml haven't changed since the last run.
        let ap_fp_dir = fingerprint::storage_dir(project_dir, target.kebab_name(), &profile_name);
        let ap_needed = annotation_processing_needed(
            &main_sources,
            &processors,
            &cache,
            project_dir,
            &generated_dir,
            &ap_fp_dir,
        );

        if !ap_needed {
            if opts.verbose {
                println!("  annotation processing: up-to-date (skipped)");
            }
        } else {
            let rt = tokio::runtime::Runtime::new().map_err(|e| KargoError::Generic {
                message: format!("Failed to create async runtime: {e}"),
            })?;
            rt.block_on(plugins::ensure_processor_jars(&processors, &cache))?;

            // --- KSP pre-build ---
            let has_ksp = processors
                .iter()
                .any(|p| p.kind == plugins::ProcessorKind::Ksp);

            if has_ksp {
                let ksp_version = plugins::resolve_ksp_version(&manifest);
                ksp_toolchain =
                    rt.block_on(plugins::ensure_ksp_toolchain(&cache, &ksp_version))?;

                if let Some(ref ksp) = ksp_toolchain {
                    let java_target_str = &preflight.java_target;

                    match ksp {
                        plugins::KspToolchain::Ksp2 { .. } => {
                            let ran = plugins::run_ksp2_standalone(
                                ksp,
                                &processors,
                                &cache,
                                &all_kotlin_dirs,
                                &cp.compile_jars,
                                &cp.processor_jars,
                                &preflight.toolchain.home,
                                &preflight.jdk.home,
                                java_target_str,
                                project_dir,
                                &generated_dir,
                                &manifest.package.name,
                                &manifest.ksp_options,
                            )?;
                            if ran && !opts.quiet {
                                status("Running", "KSP2 annotation processing");
                            }
                        }
                        plugins::KspToolchain::Ksp1 { .. } => {
                            run_ksp1_pass(
                                ksp,
                                &processors,
                                &cache,
                                &main_sources,
                                &cp.compile_jars,
                                &cp.processor_jars,
                                &preflight.toolchain.home,
                                &preflight.jdk.home,
                                project_dir,
                                &generated_dir,
                                &profile,
                                &manifest.ksp_options,
                            )?;
                            if !opts.quiet {
                                status("Running", "KSP1 annotation processing");
                            }
                        }
                    }
                }
            }

            // --- KAPT pre-build ---
            let has_kapt = processors
                .iter()
                .any(|p| p.kind == plugins::ProcessorKind::Kapt);

            if has_kapt {
                let generated = plugins::run_kapt_pass(
                    &processors,
                    &cache,
                    &main_sources,
                    &cp.compile_jars,
                    &cp.processor_jars,
                    &preflight.toolchain.home,
                    &generated_dir,
                    &profile,
                )?;
                if generated && !opts.quiet {
                    status("Running", "KAPT annotation processing");
                }
            }

            // Mark annotation processing complete so we can skip next time
            mark_annotation_processing_done(
                &main_sources,
                &processors,
                &cache,
                project_dir,
                &ap_fp_dir,
            );
        }
    }

    // 12. Build main compilation unit
    let mut compile_classpath = cp.compile_jars.clone();

    // Kotlin stdlib from the toolchain (always needed for compilation)
    let kotlin_lib = preflight.toolchain.home.join("lib");
    for jar_name in &[
        "kotlin-stdlib.jar",
        "kotlin-stdlib-jdk8.jar",
        "kotlin-stdlib-jdk7.jar",
    ] {
        let jar = kotlin_lib.join(jar_name);
        if jar.is_file()
            && !compile_classpath
                .iter()
                .any(|p| p.file_name() == jar.file_name())
        {
            compile_classpath.push(jar);
        }
    }

    // Auto-detect Kotlin compiler plugins from dependencies
    let mut compiler_args = profile.compiler_args.clone();
    detect_compiler_plugins(&lockfile, &preflight.toolchain.home, &mut compiler_args);

    // Check if KAPT generated Java sources that need javac compilation
    let kapt_sources_dir = generated_dir.join("kapt").join("sources");
    let has_kapt_java = kapt_sources_dir.is_dir() && plugins::walkdir_has_java(&kapt_sources_dir);

    // Collect processor JAR paths for fingerprinting
    let processor_jar_paths: Vec<PathBuf> = processors
        .iter()
        .filter_map(|p| cache.get_jar(&p.group, &p.artifact, &p.version, None))
        .collect();

    let main_unit = CompilationUnit {
        name: "main".into(),
        target,
        sources: main_sources,
        resource_dirs: discovered
            .main_sources
            .iter()
            .flat_map(|ss| ss.resource_dirs.clone())
            .collect(),
        classpath: compile_classpath,
        output_dir: classes_dir.clone(),
        compiler_args,
        is_test: false,
        generated_sources: generated_source_dirs,
        processor_jars: processor_jar_paths,
    };

    // 13. Unit graph (main only for `kargo build`)
    let mut graph = UnitGraph::new();
    graph.add_unit(main_unit.clone());

    // 14. Incremental build check (fingerprints stored in .kargo/)
    let fp_dir = fingerprint::storage_dir(project_dir, target.kebab_name(), &profile_name);
    let decision = incremental::check(&main_unit, &fp_dir, &kotlin_ver);
    let mut compiled = false;

    match decision {
        IncrementalDecision::UpToDate => {
            if opts.verbose {
                println!("  main: up-to-date (skipped)");
            }
        }
        IncrementalDecision::NeedsRebuild(fp) => {
            let build_cache = BuildCache::new(BuildCache::default_path(), None);
            if build_cache.restore(&fp, &classes_dir)? {
                if opts.verbose {
                    println!("  main: restored from cache");
                }
                incremental::mark_complete(&fp_dir, "main", &fp, &main_unit)?;
                compiled = true;
            } else {
                let compiler = CompilerDispatch::resolve(
                    target,
                    preflight.toolchain.clone(),
                    preflight.jdk.home.clone(),
                    preflight.java_target.clone(),
                );

                let output = compiler.compile(&main_unit, &env)?;

                if !output.success {
                    print_diagnostics(&output.diagnostics);
                    return Ok(BuildResult {
                        target,
                        profile_name,
                        build_dir,
                        classes_dir,
                        output_jar: None,
                        success: false,
                    });
                }

                if !output.diagnostics.is_empty() && opts.verbose {
                    print_diagnostics(&output.diagnostics);
                }

                // Compile KAPT-generated Java sources with javac (kotlinc doesn't compile them)
                if has_kapt_java {
                    compile_kapt_java(
                        &preflight.jdk.home,
                        &kapt_sources_dir,
                        &classes_dir,
                        &main_unit.classpath,
                        &preflight.java_target,
                    )?;
                }

                incremental::mark_complete(&fp_dir, "main", &fp, &main_unit)?;
                let _ = build_cache.put(&fp, &classes_dir);
                compiled = true;
            }
        }
    }

    // 15. Copy resources
    copy_resources(&main_unit.resource_dirs, &resources_dir);

    // 16. Package output JAR (skip if nothing was compiled)
    let output_jar = if compiled {
        let output_dir = build_dir.join("output");
        std::fs::create_dir_all(&output_dir).map_err(KargoError::Io)?;
        let jar_name = format!("{}-{}.jar", manifest.package.name, manifest.package.version);
        let jar_path = output_dir.join(&jar_name);
        package_jar(
            &preflight.jdk.home,
            &classes_dir,
            &resources_dir,
            &jar_path,
            manifest.package.main_class.as_deref(),
        )?
    } else {
        let jar_name = format!("{}-{}.jar", manifest.package.name, manifest.package.version);
        let jar_path = build_dir.join("output").join(&jar_name);
        if jar_path.is_file() {
            Some(jar_path)
        } else {
            None
        }
    };

    // 17. Print summary (suppressed in quiet mode)
    if !opts.quiet {
        let elapsed = start.elapsed();
        let file_count = main_unit.sources.len();
        if compiled {
            status(
                "Finished",
                &format!(
                    "{file_count} source file(s) [{} {}] in {:.2}s",
                    target,
                    profile_name,
                    elapsed.as_secs_f64()
                ),
            );
        } else {
            status(
                "Finished",
                &format!(
                    "up-to-date [{} {}] in {:.2}s",
                    target,
                    profile_name,
                    elapsed.as_secs_f64()
                ),
            );
        }

        if let Some(ref jar) = output_jar {
            kargo_util::progress::status_info("Output", &jar.display().to_string());
        }

        if opts.timings {
            eprintln!("  Timing breakdown:");
            eprintln!("    total: {:.2}s", elapsed.as_secs_f64());
        }
    }

    Ok(BuildResult {
        target,
        profile_name,
        build_dir,
        classes_dir,
        output_jar,
        success: true,
    })
}

fn resolve_target(manifest: &Manifest, target_arg: Option<&str>) -> miette::Result<KotlinTarget> {
    let target_name = target_arg
        .or_else(|| manifest.targets.keys().next().map(|s| s.as_str()))
        .unwrap_or("jvm");

    KotlinTarget::parse(target_name).ok_or_else(|| {
        KargoError::Generic {
            message: format!(
                "Unknown target '{}'. Available targets: {}",
                target_name,
                manifest
                    .targets
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
        .into()
    })
}

fn resolve_profile(profile_arg: Option<&str>, release: bool) -> String {
    if let Some(p) = profile_arg {
        p.to_string()
    } else if release {
        "release".to_string()
    } else {
        "dev".to_string()
    }
}

fn copy_resources(resource_dirs: &[PathBuf], target: &Path) {
    for dir in resource_dirs {
        if !dir.is_dir() {
            continue;
        }
        copy_dir_contents(dir, target);
    }
}

fn copy_dir_contents(src: &Path, dst: &Path) {
    let Ok(entries) = std::fs::read_dir(src) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let dest = dst.join(entry.file_name());
        if path.is_dir() {
            let _ = std::fs::create_dir_all(&dest);
            copy_dir_contents(&path, &dest);
        } else {
            let _ = std::fs::copy(&path, &dest);
        }
    }
}

/// Package compiled classes and resources into a JAR using `jar` from the JDK.
fn package_jar(
    jdk_home: &Path,
    classes_dir: &Path,
    resources_dir: &Path,
    jar_path: &Path,
    main_class: Option<&str>,
) -> miette::Result<Option<PathBuf>> {
    let jar_bin = jdk_home.join("bin").join("jar");
    if !jar_bin.is_file() {
        return Ok(None);
    }

    // Only package if there are actual class files
    let has_classes = classes_dir.is_dir()
        && std::fs::read_dir(classes_dir)
            .map(|rd| rd.flatten().next().is_some())
            .unwrap_or(false);
    if !has_classes {
        return Ok(None);
    }

    let mut args = vec!["cf".to_string(), jar_path.to_string_lossy().to_string()];

    // If main class is set, create a manifest with Main-Class entry
    if let Some(mc) = main_class {
        args[0] = "cfe".to_string();
        args.insert(2, mc.to_string());
    }

    // Add classes
    args.push("-C".into());
    args.push(classes_dir.to_string_lossy().to_string());
    args.push(".".into());

    // Add resources if present
    if resources_dir.is_dir()
        && std::fs::read_dir(resources_dir)
            .map(|rd| rd.flatten().next().is_some())
            .unwrap_or(false)
    {
        args.push("-C".into());
        args.push(resources_dir.to_string_lossy().to_string());
        args.push(".".into());
    }

    let cmd = kargo_util::process::CommandBuilder::new(jar_bin.to_string_lossy().to_string())
        .args(args)
        .env("JAVA_HOME", jdk_home.to_string_lossy().to_string());

    let output = cmd.exec().map_err(|e| KargoError::Generic {
        message: format!("Failed to package JAR: {e}"),
    })?;

    if output.status.success() {
        Ok(Some(jar_path.to_path_buf()))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("Warning: JAR packaging failed: {stderr}");
        Ok(None)
    }
}

/// Auto-detect Kotlin compiler plugins needed based on resolved dependencies.
///
/// Currently detected plugins:
/// - `kotlinx-serialization-compiler-plugin` when any `kotlinx-serialization-*` dep is present
pub fn detect_compiler_plugins(
    lockfile: &Lockfile,
    kotlin_home: &Path,
    compiler_args: &mut Vec<String>,
) {
    let needs_serialization = lockfile.package.iter().any(|pkg| {
        pkg.group.starts_with("org.jetbrains.kotlinx")
            && pkg.name.starts_with("kotlinx-serialization")
    });

    if needs_serialization {
        let plugin_jar = kotlin_home
            .join("lib")
            .join("kotlinx-serialization-compiler-plugin.jar");
        if plugin_jar.is_file() {
            let arg = format!("-Xplugin={}", plugin_jar.to_string_lossy());
            if !compiler_args.contains(&arg) {
                compiler_args.push(arg);
            }
        }
    }
}

/// Run KSP1 as a separate `kotlinc` pass with `-Xplugin`.
///
/// This compiles the annotated sources with the KSP1 compiler plugin,
/// which generates `.kt` files into `generated_dir/ksp/kotlin/`.
/// The compilation output is discarded — only the generated sources matter.
fn run_ksp1_pass(
    ksp: &plugins::KspToolchain,
    processors: &[plugins::ProcessorInfo],
    cache: &kargo_maven::cache::LocalCache,
    sources: &[PathBuf],
    library_jars: &[PathBuf],
    processor_scope_jars: &[PathBuf],
    kotlin_home: &Path,
    _jdk_home: &Path,
    project_dir: &Path,
    generated_dir: &Path,
    profile: &kargo_core::profile::Profile,
    ksp_options: &std::collections::BTreeMap<String, String>,
) -> miette::Result<()> {
    let ksp_args = plugins::build_ksp1_args(
        processors,
        cache,
        ksp,
        processor_scope_jars,
        generated_dir,
        project_dir,
        ksp_options,
    );
    if ksp_args.is_empty() {
        return Ok(());
    }

    let ksp_classes = generated_dir.join("ksp").join("ksp1_classes");
    std::fs::create_dir_all(&ksp_classes).map_err(KargoError::Io)?;

    let kotlinc = kotlin_home.join("bin").join("kotlinc");

    let mut cmd = kargo_util::process::CommandBuilder::new(kotlinc.to_string_lossy().to_string());

    // Add KSP plugin args
    for arg in &ksp_args {
        cmd = cmd.arg(arg);
    }

    // Add serialization plugin if needed (so KSP can analyze @Serializable types)
    for arg in &profile.compiler_args {
        if arg.contains("Xplugin") {
            cmd = cmd.arg(arg);
        }
    }
    let serial_plugin = kotlin_home
        .join("lib")
        .join("kotlinx-serialization-compiler-plugin.jar");
    if serial_plugin.is_file() {
        cmd = cmd.arg(format!("-Xplugin={}", serial_plugin.to_string_lossy()));
    }

    // Classpath
    if !library_jars.is_empty() {
        let cp = crate::classpath_string_with_stdlib(library_jars, kotlin_home);
        cmd = cmd.arg("-classpath").arg(&cp);
    }

    // Output (discarded after KSP generates sources)
    cmd = cmd.arg("-d").arg(ksp_classes.to_string_lossy().to_string());

    // Source files (exclude files that reference KSP-generated code)
    for src in sources {
        if !plugins::references_generated_imports(src) {
            cmd = cmd.arg(src.to_string_lossy().to_string());
        }
    }

    let output = cmd.exec().map_err(|e| KargoError::Generic {
        message: format!("Failed to run KSP1 pass: {e}"),
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // KSP1 pass may fail if some source files reference generated code,
        // but the important thing is that KSP generated its output.
        // Only fail if KSP itself reported errors.
        if stderr.contains("e: ") && !stderr.contains("unresolved reference") {
            return Err(KargoError::Generic {
                message: format!("KSP1 annotation processing failed:\n{stderr}"),
            }
            .into());
        }
    }

    // Clean up the throwaway class output
    let _ = std::fs::remove_dir_all(&ksp_classes);

    Ok(())
}

/// Compile KAPT-generated Java sources with `javac`.
/// `kotlinc` doesn't compile Java sources to bytecode — it only uses them
/// for type resolution. We need a separate `javac` pass that puts the
/// compiled Kotlin classes on the classpath so the generated Java code can
/// reference them.
fn compile_kapt_java(
    jdk_home: &Path,
    java_source_dir: &Path,
    classes_dir: &Path,
    classpath: &[PathBuf],
    java_target: &str,
) -> miette::Result<()> {
    let javac = jdk_home.join("bin").join("javac");
    if !javac.is_file() {
        return Err(KargoError::Generic {
            message: format!("javac not found at {}", javac.display()),
        }
        .into());
    }

    // Collect all .java files
    let mut java_files = Vec::new();
    collect_java_files(java_source_dir, &mut java_files);
    if java_files.is_empty() {
        return Ok(());
    }

    // Build classpath: compiled Kotlin classes + dependency JARs
    let mut cp_parts: Vec<String> = vec![classes_dir.to_string_lossy().to_string()];
    for jar in classpath {
        cp_parts.push(jar.to_string_lossy().to_string());
    }
    let cp = cp_parts.join(if cfg!(windows) { ";" } else { ":" });

    let mut cmd = kargo_util::process::CommandBuilder::new(javac.to_string_lossy().to_string());
    cmd = cmd
        .arg("-classpath")
        .arg(&cp)
        .arg("-d")
        .arg(classes_dir.to_string_lossy().to_string())
        .arg("-source")
        .arg(java_target)
        .arg("-target")
        .arg(java_target);

    for f in &java_files {
        cmd = cmd.arg(f.to_string_lossy().to_string());
    }

    let output = cmd.exec().map_err(|e| KargoError::Generic {
        message: format!("Failed to run javac for KAPT sources: {e}"),
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(KargoError::Generic {
            message: format!("javac compilation of KAPT-generated sources failed:\n{stderr}"),
        }
        .into());
    }

    Ok(())
}

fn collect_java_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_java_files(&p, out);
        } else if p.extension().is_some_and(|ext| ext == "java") {
            out.push(p);
        }
    }
}

fn print_diagnostics(diagnostics: &[kargo_compiler::unit::Diagnostic]) {
    use kargo_compiler::unit::DiagnosticSeverity;
    for d in diagnostics {
        let prefix = match d.severity {
            DiagnosticSeverity::Error => "error",
            DiagnosticSeverity::Warning => "warning",
            DiagnosticSeverity::Info => "info",
        };
        let location = match (&d.file, d.line) {
            (Some(f), Some(l)) => format!("{f}:{l}: "),
            (Some(f), None) => format!("{f}: "),
            _ => String::new(),
        };
        eprintln!("{location}{prefix}: {}", d.message);
    }
}

// ---------------------------------------------------------------------------
// Annotation processing (KSP/KAPT) mtime-based skip logic
// ---------------------------------------------------------------------------

/// Compute the max mtime across all annotation processing inputs:
/// source files, processor JARs, and Kargo.toml.
fn ap_inputs_max_mtime(
    sources: &[PathBuf],
    processors: &[plugins::ProcessorInfo],
    cache: &kargo_maven::cache::LocalCache,
    project_dir: &Path,
) -> u64 {
    use std::time::SystemTime;

    let mut max = 0u64;

    let mtime_of = |p: &Path| -> u64 {
        p.metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0)
    };

    for src in sources {
        max = max.max(mtime_of(src));
    }
    for p in processors {
        if let Some(jar) = cache.get_jar(&p.group, &p.artifact, &p.version, None) {
            max = max.max(mtime_of(&jar));
        }
    }
    max = max.max(mtime_of(&project_dir.join("Kargo.toml")));
    max
}

/// Check whether annotation processing needs to run.
/// Returns `false` when all inputs are older than the last successful run
/// AND the generated output directory still exists with content.
fn annotation_processing_needed(
    sources: &[PathBuf],
    processors: &[plugins::ProcessorInfo],
    cache: &kargo_maven::cache::LocalCache,
    project_dir: &Path,
    generated_dir: &Path,
    fp_dir: &Path,
) -> bool {
    // If generated output was deleted (e.g. `kargo clean`), always re-run
    if !generated_dir.is_dir() || dir_is_empty(generated_dir) {
        return true;
    }

    let marker = fp_dir.join("ap.mtime");
    let stored: u64 = std::fs::read_to_string(&marker)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    if stored == 0 {
        return true;
    }

    let current = ap_inputs_max_mtime(sources, processors, cache, project_dir);
    current > stored
}

/// Write the annotation processing mtime marker after a successful run.
fn mark_annotation_processing_done(
    sources: &[PathBuf],
    processors: &[plugins::ProcessorInfo],
    cache: &kargo_maven::cache::LocalCache,
    project_dir: &Path,
    fp_dir: &Path,
) {
    let current = ap_inputs_max_mtime(sources, processors, cache, project_dir);
    let marker = fp_dir.join("ap.mtime");
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&marker, current.to_string());
}

fn dir_is_empty(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return true;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            return false;
        }
        if path.is_dir() && !dir_is_empty(&path) {
            return false;
        }
    }
    true
}
