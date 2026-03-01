//! Operation: build the project (resolve, fetch, compile, link).
//!
//! Orchestrates the full build pipeline: preflight -> lockfile -> source discovery ->
//! classpath assembly -> KSP/KAPT -> compilation -> resource copy.
//!
//! The pipeline is split into three phases:
//! - [`run_annotation_processing`] — KSP/KAPT pre-build
//! - [`run_main_compilation`] — fingerprinting, incremental check, kotlinc + javac
//! - [`package_output`] — resource copy, JAR packaging

use std::path::{Path, PathBuf};
use std::time::Instant;

use kargo_compiler::build_cache::BuildCache;
use kargo_compiler::dispatch::CompilerDispatch;
use kargo_compiler::fingerprint;
use kargo_compiler::incremental::{self, IncrementalDecision};
use kargo_compiler::plugins;
use kargo_compiler::source_set_discovery::collect_kotlin_files;
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

/// Result of a build operation, carrying enough context for downstream ops.
pub struct BuildResult {
    pub target: KotlinTarget,
    pub profile_name: String,
    pub build_dir: PathBuf,
    pub classes_dir: PathBuf,
    /// Path to the packaged output JAR, if produced.
    pub output_jar: Option<PathBuf>,
    pub success: bool,
    /// Manifest loaded during build (avoids re-parsing in test/run).
    pub manifest: Manifest,
    /// Lockfile loaded during build (avoids re-parsing in test/run).
    pub lockfile: Lockfile,
    /// Preflight result (toolchain info) for reuse.
    pub preflight: crate::ops_setup::PreflightResult,
}

/// Output from the compilation phase.
struct CompilationOutput {
    compiled: bool,
    main_unit: CompilationUnit,
}

/// Run the full build pipeline.
pub fn build(project_dir: &Path, opts: &BuildOptions) -> miette::Result<BuildResult> {
    let start = Instant::now();
    use kargo_util::progress::status;

    let ctx = crate::BuildContext::load(
        project_dir,
        opts.target.as_deref(),
        opts.profile.as_deref(),
        opts.release,
    )?;

    if opts.verbose {
        ops_setup::print_preflight_summary(&ctx.preflight);
        println!();
    }

    let target = ctx.target;
    let profile_name = ctx.profile_name.clone();

    if !opts.quiet {
        status(
            "Compiling",
            &format!(
                "{} v{} ({} {})",
                ctx.manifest.package.name, ctx.manifest.package.version, target, profile_name
            ),
        );
    }

    // Collect main source files
    let mut all_kotlin_dirs: Vec<PathBuf> = Vec::new();
    for ss in &ctx.discovered.main_sources {
        all_kotlin_dirs.extend(ss.kotlin_dirs.clone());
    }
    let main_sources = collect_kotlin_files(&all_kotlin_dirs);

    if main_sources.is_empty() {
        println!("No Kotlin source files found to compile.");
        return Ok(BuildResult {
            target,
            profile_name,
            build_dir: ctx.build_dir.clone(),
            classes_dir: ctx.classes_dir.clone(),
            output_jar: None,
            success: true,
            manifest: ctx.manifest,
            lockfile: ctx.lockfile,
            preflight: ctx.preflight,
        });
    }

    // Generate BuildConfig.kt
    generate_build_config(&ctx, &profile_name)?;

    // Phase 1: Annotation processing
    let cache = kargo_maven::cache::LocalCache::new(project_dir);
    let processors = plugins::detect_processors(&ctx.manifest, &cache);

    run_annotation_processing(
        &ctx,
        &processors,
        &main_sources,
        &all_kotlin_dirs,
        &cache,
        opts,
    )?;

    // Phase 2: Main compilation
    let comp_output = run_main_compilation(
        &ctx,
        &processors,
        &main_sources,
        &cache,
        opts,
    )?;

    if !comp_output.compiled && !comp_output.main_unit.sources.is_empty() {
        // Check for failed build
    }

    // Phase 3: Package output
    let output_jar = package_output(&ctx, comp_output.compiled)?;

    // Print summary
    if !opts.quiet {
        let elapsed = start.elapsed();
        let file_count = comp_output.main_unit.sources.len();
        if comp_output.compiled {
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
        build_dir: ctx.build_dir.clone(),
        classes_dir: ctx.classes_dir.clone(),
        output_jar,
        success: true,
        manifest: ctx.manifest,
        lockfile: ctx.lockfile,
        preflight: ctx.preflight,
    })
}

// ---------------------------------------------------------------------------
// Phase 1: Annotation processing (KSP/KAPT)
// ---------------------------------------------------------------------------

fn run_annotation_processing(
    ctx: &crate::BuildContext,
    processors: &[plugins::ProcessorInfo],
    main_sources: &[PathBuf],
    all_kotlin_dirs: &[PathBuf],
    cache: &kargo_maven::cache::LocalCache,
    opts: &BuildOptions,
) -> miette::Result<()> {
    use kargo_util::progress::status;

    if processors.is_empty() {
        return Ok(());
    }

    let ap_fp_dir =
        fingerprint::storage_dir(&ctx.project_dir, ctx.target.kebab_name(), &ctx.profile_name);
    let ap_needed = annotation_processing_needed(
        main_sources,
        processors,
        cache,
        &ctx.project_dir,
        &ctx.generated_dir,
        &ap_fp_dir,
    );

    if !ap_needed {
        if opts.verbose {
            println!("  annotation processing: up-to-date (skipped)");
        }
        return Ok(());
    }

    let rt = tokio::runtime::Runtime::new().map_err(|e| KargoError::Generic {
        message: format!("Failed to create async runtime: {e}"),
    })?;
    rt.block_on(plugins::ensure_processor_jars(processors, cache))?;

    // KSP pre-build
    let has_ksp = processors
        .iter()
        .any(|p| p.kind == plugins::ProcessorKind::Ksp);

    if has_ksp {
        let ksp_version = plugins::resolve_ksp_version(&ctx.manifest);
        let ksp_toolchain = rt.block_on(plugins::ensure_ksp_toolchain(cache, &ksp_version))?;

        if let Some(ref ksp) = ksp_toolchain {
            match ksp {
                plugins::KspToolchain::Ksp2 { .. } => {
                    let ran = plugins::run_ksp2_standalone(
                        ksp,
                        processors,
                        cache,
                        all_kotlin_dirs,
                        &ctx.classpath.compile_jars,
                        &ctx.classpath.processor_jars,
                        &ctx.preflight.toolchain.home,
                        &ctx.preflight.jdk.home,
                        &ctx.preflight.java_target,
                        &ctx.project_dir,
                        &ctx.generated_dir,
                        &ctx.manifest.package.name,
                        &ctx.manifest.ksp_options,
                    )?;
                    if ran && !opts.quiet {
                        status("Running", "KSP2 annotation processing");
                    }
                }
                plugins::KspToolchain::Ksp1 { .. } => {
                    run_ksp1_pass(
                        ksp,
                        processors,
                        cache,
                        main_sources,
                        &ctx.classpath.compile_jars,
                        &ctx.classpath.processor_jars,
                        &ctx.preflight.toolchain.home,
                        &ctx.preflight.jdk.home,
                        &ctx.project_dir,
                        &ctx.generated_dir,
                        &ctx.profile,
                        &ctx.manifest.ksp_options,
                    )?;
                    if !opts.quiet {
                        status("Running", "KSP1 annotation processing");
                    }
                }
            }
        }
    }

    // KAPT pre-build
    let has_kapt = processors
        .iter()
        .any(|p| p.kind == plugins::ProcessorKind::Kapt);

    if has_kapt {
        let generated = plugins::run_kapt_pass(
            processors,
            cache,
            main_sources,
            &ctx.classpath.compile_jars,
            &ctx.classpath.processor_jars,
            &ctx.preflight.toolchain.home,
            &ctx.generated_dir,
            &ctx.profile,
        )?;
        if generated && !opts.quiet {
            status("Running", "KAPT annotation processing");
        }
    }

    mark_annotation_processing_done(main_sources, processors, cache, &ctx.project_dir, &ap_fp_dir);
    Ok(())
}

// ---------------------------------------------------------------------------
// Phase 2: Main compilation
// ---------------------------------------------------------------------------

fn run_main_compilation(
    ctx: &crate::BuildContext,
    processors: &[plugins::ProcessorInfo],
    main_sources: &[PathBuf],
    cache: &kargo_maven::cache::LocalCache,
    opts: &BuildOptions,
) -> miette::Result<CompilationOutput> {
    let mut compile_classpath = ctx.classpath.compile_jars.clone();

    let kotlin_lib = ctx.preflight.toolchain.home.join("lib");
    for jar_name in kargo_compiler::classpath::STDLIB_RUNTIME_JARS {
        let jar = kotlin_lib.join(jar_name);
        if jar.is_file()
            && !compile_classpath
                .iter()
                .any(|p| p.file_name() == jar.file_name())
        {
            compile_classpath.push(jar);
        }
    }

    let mut compiler_args = ctx.profile.compiler_args.clone();
    detect_compiler_plugins(&ctx.lockfile, &ctx.preflight.toolchain.home, &mut compiler_args);

    let kapt_sources_dir = ctx.generated_dir.join("kapt").join("sources");
    let has_kapt_java = kapt_sources_dir.is_dir() && plugins::walkdir_has_java(&kapt_sources_dir);

    let processor_jar_paths: Vec<PathBuf> = processors
        .iter()
        .filter_map(|p| cache.get_jar(&p.group, &p.artifact, &p.version, None))
        .collect();

    let main_unit = CompilationUnit {
        name: "main".into(),
        target: ctx.target,
        sources: main_sources.to_vec(),
        resource_dirs: ctx
            .discovered
            .main_sources
            .iter()
            .flat_map(|ss| ss.resource_dirs.clone())
            .collect(),
        classpath: compile_classpath,
        output_dir: ctx.classes_dir.clone(),
        compiler_args,
        is_test: false,
        generated_sources: vec![ctx.generated_dir.clone()],
        processor_jars: processor_jar_paths,
    };

    let mut graph = UnitGraph::new();
    graph.add_unit(main_unit.clone());

    let kotlin_ver = ctx.preflight.toolchain.version.to_string();
    let fp_dir =
        fingerprint::storage_dir(&ctx.project_dir, ctx.target.kebab_name(), &ctx.profile_name);
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
            if build_cache.restore(&fp, &ctx.classes_dir)? {
                if opts.verbose {
                    println!("  main: restored from cache");
                }
                incremental::mark_complete(&fp_dir, "main", &fp, &main_unit)?;
                compiled = true;
            } else {
                let compiler = CompilerDispatch::resolve(
                    ctx.target,
                    ctx.preflight.toolchain.clone(),
                    ctx.preflight.jdk.home.clone(),
                    ctx.preflight.java_target.clone(),
                );

                let output = compiler.compile(&main_unit, &ctx.env)?;

                if !output.success {
                    print_diagnostics(&output.diagnostics);
                    return Err(KargoError::Generic {
                        message: "Compilation failed.".into(),
                    }
                    .into());
                }

                if !output.diagnostics.is_empty() && opts.verbose {
                    print_diagnostics(&output.diagnostics);
                }

                if has_kapt_java {
                    compile_kapt_java(
                        &ctx.preflight.jdk.home,
                        &kapt_sources_dir,
                        &ctx.classes_dir,
                        &main_unit.classpath,
                        &ctx.preflight.java_target,
                    )?;
                }

                incremental::mark_complete(&fp_dir, "main", &fp, &main_unit)?;
                let _ = build_cache.put(&fp, &ctx.classes_dir);
                compiled = true;
            }
        }
    }

    Ok(CompilationOutput {
        compiled,
        main_unit,
    })
}

// ---------------------------------------------------------------------------
// Phase 3: Package output
// ---------------------------------------------------------------------------

fn package_output(
    ctx: &crate::BuildContext,
    compiled: bool,
) -> miette::Result<Option<PathBuf>> {
    // Copy resources
    let resource_dirs: Vec<PathBuf> = ctx
        .discovered
        .main_sources
        .iter()
        .flat_map(|ss| ss.resource_dirs.clone())
        .collect();
    copy_resources(&resource_dirs, &ctx.resources_dir);

    if compiled {
        let output_dir = ctx.build_dir.join("output");
        std::fs::create_dir_all(&output_dir).map_err(KargoError::Io)?;
        let jar_name = format!(
            "{}-{}.jar",
            ctx.manifest.package.name, ctx.manifest.package.version
        );
        let jar_path = output_dir.join(&jar_name);
        package_jar(
            &ctx.preflight.jdk.home,
            &ctx.classes_dir,
            &ctx.resources_dir,
            &jar_path,
            ctx.manifest.package.main_class.as_deref(),
        )
    } else {
        let jar_name = format!(
            "{}-{}.jar",
            ctx.manifest.package.name, ctx.manifest.package.version
        );
        let jar_path = ctx.build_dir.join("output").join(&jar_name);
        if jar_path.is_file() {
            Ok(Some(jar_path))
        } else {
            Ok(None)
        }
    }
}

// ---------------------------------------------------------------------------
// BuildConfig generation
// ---------------------------------------------------------------------------

fn generate_build_config(
    ctx: &crate::BuildContext,
    profile_name: &str,
) -> miette::Result<PathBuf> {
    let is_debug = ctx.profile.debug.unwrap_or(profile_name != "release");

    let kotlin_package = ctx
        .manifest
        .package
        .group
        .clone()
        .or_else(|| {
            ctx.manifest
                .package
                .main_class
                .as_deref()
                .and_then(kargo_compiler::buildconfig::package_from_main_class)
        });

    let mut build_config_fields = ctx.manifest.build_config.clone();
    if let Some(ref flavors) = ctx.manifest.flavors {
        let selected: std::collections::BTreeMap<String, String> =
            flavors.default.clone().unwrap_or_default();

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

    kargo_compiler::buildconfig::generate(
        &ctx.generated_dir,
        kotlin_package.as_deref(),
        &ctx.manifest.package.name,
        &ctx.manifest.package.version,
        profile_name,
        is_debug,
        &build_config_fields,
    )
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

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

    let has_classes = classes_dir.is_dir()
        && std::fs::read_dir(classes_dir)
            .map(|rd| rd.flatten().next().is_some())
            .unwrap_or(false);
    if !has_classes {
        return Ok(None);
    }

    let mut args = vec!["cf".to_string(), jar_path.to_string_lossy().to_string()];

    if let Some(mc) = main_class {
        args[0] = "cfe".to_string();
        args.insert(2, mc.to_string());
    }

    args.push("-C".into());
    args.push(classes_dir.to_string_lossy().to_string());
    args.push(".".into());

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

    for arg in &ksp_args {
        cmd = cmd.arg(arg);
    }

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

    if !library_jars.is_empty() {
        let cp = crate::classpath_string_with_stdlib(library_jars, kotlin_home);
        cmd = cmd.arg("-classpath").arg(&cp);
    }

    cmd = cmd.arg("-d").arg(ksp_classes.to_string_lossy().to_string());

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
        if stderr.contains("e: ") && !stderr.contains("unresolved reference") {
            return Err(KargoError::Generic {
                message: format!("KSP1 annotation processing failed:\n{stderr}"),
            }
            .into());
        }
    }

    let _ = std::fs::remove_dir_all(&ksp_classes);

    Ok(())
}

/// Compile KAPT-generated Java sources with `javac`.
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

    let mut java_files = Vec::new();
    collect_java_files(java_source_dir, &mut java_files);
    if java_files.is_empty() {
        return Ok(());
    }

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
// Annotation processing mtime-based skip logic
// ---------------------------------------------------------------------------

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

fn annotation_processing_needed(
    sources: &[PathBuf],
    processors: &[plugins::ProcessorInfo],
    cache: &kargo_maven::cache::LocalCache,
    project_dir: &Path,
    generated_dir: &Path,
    fp_dir: &Path,
) -> bool {
    // Check if the processor-specific output dirs exist. The top-level
    // generated_dir may contain BuildConfig.kt (written before AP runs),
    // so we must check for actual AP output: kapt/sources or ksp/kotlin.
    let has_kapt = processors
        .iter()
        .any(|p| p.kind == plugins::ProcessorKind::Kapt);
    let has_ksp = processors
        .iter()
        .any(|p| p.kind == plugins::ProcessorKind::Ksp);

    let kapt_output = generated_dir.join("kapt").join("sources");
    let ksp_output = generated_dir.join("ksp").join("kotlin");

    let ap_output_exists = (has_kapt && kapt_output.is_dir() && !dir_is_empty(&kapt_output))
        || (has_ksp && ksp_output.is_dir() && !dir_is_empty(&ksp_output));

    if !ap_output_exists {
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
