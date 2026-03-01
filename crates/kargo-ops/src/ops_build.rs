//! Operation: build the project (resolve, fetch, compile, link).
//!
//! Orchestrates the full build pipeline: preflight -> lockfile -> source discovery ->
//! classpath assembly -> KSP/KAPT -> compilation -> resource copy.
//!
//! The pipeline is split into three phases:
//! - [`run_annotation_processing`] — KSP/KAPT pre-build
//! - [`run_main_compilation`] — fingerprinting, incremental check, kotlinc + javac
//! - [`package_output`] — resource copy, JAR packaging

use std::collections::HashMap;
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
    /// Assembled classpath for reuse by run/test.
    pub classpath: kargo_compiler::classpath::Classpath,
    /// Discovered source sets for reuse by test.
    pub discovered: kargo_compiler::source_set_discovery::DiscoveredSources,
}

/// Output from the compilation phase.
struct CompilationOutput {
    compiled: bool,
    main_unit: CompilationUnit,
}

/// Run the full build pipeline.
pub async fn build(project_dir: &Path, opts: &BuildOptions) -> miette::Result<BuildResult> {
    let start = Instant::now();
    use kargo_util::progress::status;

    let ctx = crate::BuildContext::load(
        project_dir,
        opts.target.as_deref(),
        opts.profile.as_deref(),
        opts.release,
    )
    .await?;

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
            classpath: ctx.classpath,
            discovered: ctx.discovered,
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
    )
    .await?;

    // Phase 2: Main compilation
    let comp_output = run_main_compilation(&ctx, &processors, &main_sources, &cache, opts)?;

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
        classpath: ctx.classpath,
        discovered: ctx.discovered,
    })
}

// ---------------------------------------------------------------------------
// Phase 1: Annotation processing (KSP/KAPT)
// ---------------------------------------------------------------------------

async fn run_annotation_processing(
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
    let decision = annotation_processing_decision(
        main_sources,
        processors,
        cache,
        &ctx.project_dir,
        &ctx.generated_dir,
        &ap_fp_dir,
    );

    let changed_files = match decision {
        ApDecision::UpToDate => {
            if opts.verbose {
                println!("  annotation processing: up-to-date (skipped)");
            }
            return Ok(());
        }
        ApDecision::FullRun => None,
        ApDecision::Incremental(files) => Some(files),
    };

    plugins::ensure_processor_jars(processors, cache).await?;

    // KSP pre-build
    let has_ksp = processors
        .iter()
        .any(|p| p.kind == plugins::ProcessorKind::Ksp);

    if has_ksp {
        let ksp_version = plugins::resolve_ksp_version(&ctx.manifest);
        let ksp_toolchain = plugins::ensure_ksp_toolchain(cache, &ksp_version).await?;

        if let Some(ref ksp) = ksp_toolchain {
            let ksp_ap = plugins::ApContext {
                processors,
                cache,
                sources: all_kotlin_dirs,
                library_jars: &ctx.classpath.compile_jars,
                processor_scope_jars: &ctx.classpath.processor_jars,
                kotlin_home: &ctx.preflight.toolchain.home,
                jdk_home: &ctx.preflight.jdk.home,
                project_dir: &ctx.project_dir,
                generated_dir: &ctx.generated_dir,
            };

            match ksp {
                plugins::KspToolchain::Ksp2 { .. } => {
                    let ran = plugins::run_ksp2_standalone(
                        ksp,
                        &ksp_ap,
                        &ctx.preflight.java_target,
                        &ctx.manifest.package.name,
                        &ctx.manifest.ksp_options,
                        changed_files.as_deref(),
                    )?;
                    let mode = if changed_files.is_some() {
                        "KSP2 annotation processing (incremental)"
                    } else {
                        "KSP2 annotation processing"
                    };
                    if ran && !opts.quiet {
                        status("Running", mode);
                    }
                }
                plugins::KspToolchain::Ksp1 { .. } => {
                    let ksp1_ap = plugins::ApContext {
                        sources: main_sources,
                        ..ksp_ap
                    };
                    run_ksp1_pass(ksp, &ksp1_ap, &ctx.profile, &ctx.manifest.ksp_options)?;
                    if !opts.quiet {
                        status("Running", "KSP1 annotation processing");
                    }
                }
            }
        }
    }

    // KAPT pre-build (no incremental support — always full pass)
    let has_kapt = processors
        .iter()
        .any(|p| p.kind == plugins::ProcessorKind::Kapt);

    if has_kapt {
        let kapt_ap = plugins::ApContext {
            processors,
            cache,
            sources: main_sources,
            library_jars: &ctx.classpath.compile_jars,
            processor_scope_jars: &ctx.classpath.processor_jars,
            kotlin_home: &ctx.preflight.toolchain.home,
            jdk_home: &ctx.preflight.jdk.home,
            project_dir: &ctx.project_dir,
            generated_dir: &ctx.generated_dir,
        };
        let generated = plugins::run_kapt_pass(&kapt_ap, &ctx.profile)?;
        if generated && !opts.quiet {
            status("Running", "KAPT annotation processing");
        }
    }

    mark_annotation_processing_done(
        main_sources,
        processors,
        cache,
        &ctx.project_dir,
        &ap_fp_dir,
    );
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
    detect_compiler_plugins(
        &ctx.lockfile,
        &ctx.preflight.toolchain.home,
        &mut compiler_args,
    );

    let kapt_sources_dir = ctx.generated_dir.join("kapt").join("sources");
    let has_kapt_java = kapt_sources_dir.is_dir() && plugins::walkdir_has_java(&kapt_sources_dir);

    let processor_jar_paths: Vec<PathBuf> = processors
        .iter()
        .filter_map(|p| cache.get_jar(&p.group, &p.artifact, &p.version, None))
        .collect();

    let (gen_dirs, gen_files) = collect_generated_sources(&ctx.generated_dir);
    let mut all_main_sources = main_sources.to_vec();
    all_main_sources.extend(gen_files);

    let main_unit = CompilationUnit {
        name: "main".into(),
        target: ctx.target,
        sources: all_main_sources,
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
        generated_sources: gen_dirs,
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

fn package_output(ctx: &crate::BuildContext, compiled: bool) -> miette::Result<Option<PathBuf>> {
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

fn generate_build_config(ctx: &crate::BuildContext, profile_name: &str) -> miette::Result<PathBuf> {
    let is_debug = ctx.profile.debug.unwrap_or(profile_name != "release");

    let kotlin_package = ctx.manifest.package.group.clone().or_else(|| {
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
            if let Err(e) = std::fs::create_dir_all(&dest) {
                tracing::warn!("Failed to create directory {}: {e}", dest.display());
            }
            copy_dir_contents(&path, &dest);
        } else if let Err(e) = std::fs::copy(&path, &dest) {
            tracing::warn!(
                "Failed to copy {} to {}: {e}",
                path.display(),
                dest.display()
            );
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
    ap: &plugins::ApContext<'_>,
    profile: &kargo_core::profile::Profile,
    ksp_options: &std::collections::BTreeMap<String, String>,
) -> miette::Result<()> {
    let ksp_args = plugins::build_ksp1_args(
        ap.processors,
        ap.cache,
        ksp,
        ap.processor_scope_jars,
        ap.generated_dir,
        ap.project_dir,
        ksp_options,
    );
    if ksp_args.is_empty() {
        return Ok(());
    }

    let ksp_classes = ap.generated_dir.join("ksp").join("ksp1_classes");
    std::fs::create_dir_all(&ksp_classes).map_err(KargoError::Io)?;

    let kotlinc = ap.kotlin_home.join("bin").join("kotlinc");
    let mut cmd = kargo_util::process::CommandBuilder::new(kotlinc.to_string_lossy().to_string());

    for arg in &ksp_args {
        cmd = cmd.arg(arg);
    }

    for arg in &profile.compiler_args {
        if arg.contains("Xplugin") {
            cmd = cmd.arg(arg);
        }
    }
    let serial_plugin = ap
        .kotlin_home
        .join("lib")
        .join("kotlinx-serialization-compiler-plugin.jar");
    if serial_plugin.is_file() {
        cmd = cmd.arg(format!("-Xplugin={}", serial_plugin.to_string_lossy()));
    }

    if !ap.library_jars.is_empty() {
        let cp = crate::classpath_string_with_stdlib(ap.library_jars, ap.kotlin_home);
        cmd = cmd.arg("-classpath").arg(&cp);
    }

    cmd = cmd.arg("-d").arg(ksp_classes.to_string_lossy().to_string());

    for src in ap.sources {
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

    if let Err(e) = std::fs::remove_dir_all(&ksp_classes) {
        tracing::warn!(
            "Failed to remove KSP classes directory {}: {e}",
            ksp_classes.display()
        );
    }

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

/// Collect generated source directories and individual files for compilation.
/// Returns `(directories, individual_files)`.
///
/// Only includes specific known output directories (ksp/kotlin, ksp/java,
/// kapt/sources) to avoid recursing into KSP2 internal directories
/// (caches, backups) that would cause duplicate declarations.
fn collect_generated_sources(generated_dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut dirs = Vec::new();
    let mut files = Vec::new();

    let ksp_kotlin = generated_dir.join("ksp").join("kotlin");
    if ksp_kotlin.is_dir() {
        dirs.push(ksp_kotlin);
    }
    let ksp_java = generated_dir.join("ksp").join("java");
    if ksp_java.is_dir() {
        dirs.push(ksp_java);
    }

    let kapt_sources = generated_dir.join("kapt").join("sources");
    if kapt_sources.is_dir() {
        dirs.push(kapt_sources);
    }

    // Top-level files (e.g., BuildConfig.kt) — added individually to avoid
    // recursing into the entire generated_dir.
    if let Ok(entries) = std::fs::read_dir(generated_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext == "kt" || ext == "java")
            {
                files.push(path);
            }
        }
    }

    if dirs.is_empty() && files.is_empty() {
        dirs.push(generated_dir.to_path_buf());
    }

    (dirs, files)
}

// ---------------------------------------------------------------------------
// Annotation processing: two-tier skip logic (mtime + content fingerprint)
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

/// Per-file content hashes for AP inputs plus a composite fingerprint.
struct ApFingerprint {
    composite: String,
    file_hashes: HashMap<String, String>,
}

/// Content-based fingerprint of all AP inputs: source file contents,
/// processor JAR filenames, and Kargo.toml contents.
fn ap_inputs_fingerprint(
    sources: &[PathBuf],
    processors: &[plugins::ProcessorInfo],
    cache: &kargo_maven::cache::LocalCache,
    project_dir: &Path,
) -> ApFingerprint {
    use kargo_util::hash::sha256_bytes;

    let mut parts: Vec<String> = Vec::new();
    let mut file_hashes: HashMap<String, String> = HashMap::new();

    let mut sorted_sources: Vec<&PathBuf> = sources.iter().collect();
    sorted_sources.sort();
    for src in sorted_sources {
        if let Ok(content) = std::fs::read(src) {
            let h = sha256_bytes(&content);
            let key = src.to_string_lossy().to_string();
            parts.push(format!("src:{key}:{h}"));
            file_hashes.insert(key, h);
        }
    }

    let mut proc_entries: Vec<String> = processors
        .iter()
        .filter_map(|p| {
            cache
                .get_jar(&p.group, &p.artifact, &p.version, None)
                .and_then(|jar| jar.file_name().map(|f| f.to_string_lossy().to_string()))
        })
        .collect();
    proc_entries.sort();
    for jar in &proc_entries {
        parts.push(format!("proc:{jar}"));
    }

    let manifest_path = project_dir.join("Kargo.toml");
    if let Ok(content) = std::fs::read(&manifest_path) {
        let h = sha256_bytes(&content);
        parts.push(format!("manifest:{h}"));
    }

    let combined = parts.join("\n");
    ApFingerprint {
        composite: sha256_bytes(combined.as_bytes()),
        file_hashes,
    }
}

fn load_ap_file_hashes(fp_dir: &Path) -> HashMap<String, String> {
    let path = fp_dir.join("ap.file_hashes");
    let Ok(content) = std::fs::read_to_string(&path) else {
        return HashMap::new();
    };
    content
        .lines()
        .filter_map(|line| {
            let (path_str, hash) = line.split_once('\t')?;
            Some((path_str.to_string(), hash.to_string()))
        })
        .collect()
}

fn save_ap_file_hashes(fp_dir: &Path, hashes: &HashMap<String, String>) {
    let path = fp_dir.join("ap.file_hashes");
    let mut entries: Vec<(&str, &str)> = hashes
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    entries.sort_by_key(|(k, _)| *k);
    let content: String = entries
        .iter()
        .map(|(k, v)| format!("{k}\t{v}"))
        .collect::<Vec<_>>()
        .join("\n");
    if let Err(e) = std::fs::write(&path, &content) {
        tracing::warn!("Failed to write AP file hashes {}: {e}", path.display());
    }
}

/// Compute which source files changed compared to the stored per-file hashes.
/// Returns `None` if there are no stored hashes (first build / clean).
fn compute_changed_sources(
    sources: &[PathBuf],
    current_hashes: &HashMap<String, String>,
    fp_dir: &Path,
) -> Option<Vec<PathBuf>> {
    let stored = load_ap_file_hashes(fp_dir);
    if stored.is_empty() {
        return None;
    }

    let mut changed = Vec::new();
    for src in sources {
        let key = src.to_string_lossy().to_string();
        let current = current_hashes.get(&key);
        let previous = stored.get(&key);
        match (current, previous) {
            (Some(c), Some(p)) if c == p => {} // unchanged
            _ => changed.push(src.clone()),    // new, removed, or changed
        }
    }

    // Also detect removed files (in stored but not in current)
    for stored_key in stored.keys() {
        if !current_hashes.contains_key(stored_key) {
            changed.push(PathBuf::from(stored_key));
        }
    }

    Some(changed)
}

/// Result of the AP skip check: either skip entirely, run a full pass, or
/// run an incremental pass with a list of changed source files.
enum ApDecision {
    UpToDate,
    FullRun,
    Incremental(Vec<PathBuf>),
}

fn annotation_processing_decision(
    sources: &[PathBuf],
    processors: &[plugins::ProcessorInfo],
    cache: &kargo_maven::cache::LocalCache,
    project_dir: &Path,
    generated_dir: &Path,
    fp_dir: &Path,
) -> ApDecision {
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
        return ApDecision::FullRun;
    }

    // Fast path: mtime comparison + source count check.
    // File removal doesn't increase max mtime, so we also compare the number
    // of source files against the stored per-file hash count.
    let mtime_marker = fp_dir.join("ap.mtime");
    let stored_mtime: u64 = std::fs::read_to_string(&mtime_marker)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    if stored_mtime == 0 {
        return ApDecision::FullRun;
    }

    let stored_file_count = load_ap_file_hashes(fp_dir).len();
    let current_mtime = ap_inputs_max_mtime(sources, processors, cache, project_dir);
    if current_mtime <= stored_mtime && sources.len() == stored_file_count {
        return ApDecision::UpToDate;
    }

    // Slow path: mtime changed, but check if content actually changed.
    let fp_marker = fp_dir.join("ap.fingerprint");
    let stored_fp = std::fs::read_to_string(&fp_marker)
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    if stored_fp.is_empty() {
        return ApDecision::FullRun;
    }

    let current = ap_inputs_fingerprint(sources, processors, cache, project_dir);
    if current.composite == stored_fp {
        let _ = std::fs::write(&mtime_marker, current_mtime.to_string());
        return ApDecision::UpToDate;
    }

    // Content changed — compute which specific files changed for KSP2 incremental
    match compute_changed_sources(sources, &current.file_hashes, fp_dir) {
        Some(changed) if !changed.is_empty() => ApDecision::Incremental(changed),
        Some(_) => ApDecision::FullRun, // hashes exist but diff is empty (shouldn't happen)
        None => ApDecision::FullRun,    // no stored hashes — first build
    }
}

fn mark_annotation_processing_done(
    sources: &[PathBuf],
    processors: &[plugins::ProcessorInfo],
    cache: &kargo_maven::cache::LocalCache,
    project_dir: &Path,
    fp_dir: &Path,
) {
    if let Err(e) = std::fs::create_dir_all(fp_dir) {
        tracing::warn!(
            "Failed to create fingerprint directory {}: {e}",
            fp_dir.display()
        );
        return;
    }

    let current_mtime = ap_inputs_max_mtime(sources, processors, cache, project_dir);
    let mtime_marker = fp_dir.join("ap.mtime");
    if let Err(e) = std::fs::write(&mtime_marker, current_mtime.to_string()) {
        tracing::warn!(
            "Failed to write AP mtime marker {}: {e}",
            mtime_marker.display()
        );
    }

    let current = ap_inputs_fingerprint(sources, processors, cache, project_dir);
    let fp_marker = fp_dir.join("ap.fingerprint");
    if let Err(e) = std::fs::write(&fp_marker, &current.composite) {
        tracing::warn!(
            "Failed to write AP fingerprint {}: {e}",
            fp_marker.display()
        );
    }

    save_ap_file_hashes(fp_dir, &current.file_hashes);
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
