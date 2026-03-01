//! KAPT (Kotlin Annotation Processing Tool) execution.

use std::path::{Path, PathBuf};

use kargo_maven::cache::LocalCache;
use kargo_util::errors::KargoError;

use super::{ProcessorInfo, ProcessorKind};
use crate::classpath::to_classpath_string;

const KAPT_PLUGIN_ID: &str = "org.jetbrains.kotlin.kapt3";

/// Run KAPT as a pre-build step, similar to how KSP runs.
/// Invokes `kotlinc` with the KAPT plugin on source files that don't reference
/// generated code. The generated Java sources end up in `generated_dir/kapt/sources/`
/// and are then included in the main compilation via `generated_source_dirs`.
pub fn run_kapt_pass(
    processors: &[ProcessorInfo],
    cache: &LocalCache,
    sources: &[PathBuf],
    library_jars: &[PathBuf],
    processor_scope_jars: &[PathBuf],
    kotlin_home: &Path,
    generated_dir: &Path,
    profile: &kargo_core::profile::Profile,
) -> miette::Result<bool> {
    let kapt_procs: Vec<&ProcessorInfo> = processors
        .iter()
        .filter(|p| p.kind == ProcessorKind::Kapt)
        .collect();

    if kapt_procs.is_empty() {
        return Ok(false);
    }

    let proc_jars: Vec<PathBuf> = kapt_procs
        .iter()
        .filter_map(|p| cache.get_jar(&p.group, &p.artifact, &p.version, None))
        .collect();

    if proc_jars.is_empty() {
        return Ok(false);
    }

    let kapt_plugin_jar = kotlin_home
        .join("lib")
        .join("kotlin-annotation-processing.jar");
    if !kapt_plugin_jar.is_file() {
        return Err(KargoError::Generic {
            message: format!("KAPT plugin JAR not found at {}", kapt_plugin_jar.display()),
        }
        .into());
    }

    let mut full_proc_cp = proc_jars;
    for jar in processor_scope_jars {
        if !full_proc_cp.contains(jar) {
            full_proc_cp.push(jar.clone());
        }
    }
    for lib_jar in library_jars {
        if !full_proc_cp.contains(lib_jar) {
            full_proc_cp.push(lib_jar.clone());
        }
    }
    let proc_classpath = full_proc_cp
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(if cfg!(windows) { ";" } else { ":" });

    let generated_sources = generated_dir.join("kapt").join("sources");
    let classes_dir = generated_dir.join("kapt").join("classes");
    let stubs_dir = generated_dir.join("kapt").join("stubs");
    for dir in [&generated_sources, &classes_dir, &stubs_dir] {
        let _ = std::fs::create_dir_all(dir);
    }

    let kotlinc = kotlin_home.join("bin").join("kotlinc");
    let mut cmd = kargo_util::process::CommandBuilder::new(kotlinc.to_string_lossy().to_string());

    cmd = cmd.arg(format!("-Xplugin={}", kapt_plugin_jar.to_string_lossy()));

    cmd = cmd.arg(format!(
        "-P=plugin:{KAPT_PLUGIN_ID}:apclasspath={proc_classpath}"
    ));
    cmd = cmd.arg(format!(
        "-P=plugin:{KAPT_PLUGIN_ID}:sources={}",
        generated_sources.to_string_lossy()
    ));
    cmd = cmd.arg(format!(
        "-P=plugin:{KAPT_PLUGIN_ID}:classes={}",
        classes_dir.to_string_lossy()
    ));
    cmd = cmd.arg(format!(
        "-P=plugin:{KAPT_PLUGIN_ID}:stubs={}",
        stubs_dir.to_string_lossy()
    ));
    cmd = cmd.arg(format!("-P=plugin:{KAPT_PLUGIN_ID}:aptMode=stubsAndApt"));

    let processor_classes = discover_processor_classes(&full_proc_cp);
    if !processor_classes.is_empty() {
        let procs_str = processor_classes.join(",");
        cmd = cmd.arg(format!(
            "-P=plugin:{KAPT_PLUGIN_ID}:processors={procs_str}"
        ));
    }

    for arg in &profile.compiler_args {
        if arg.contains("Xplugin") {
            cmd = cmd.arg(arg);
        }
    }

    let mut kapt_cp_jars: Vec<PathBuf> = library_jars.to_vec();
    for jar in processor_scope_jars {
        if !kapt_cp_jars.contains(jar) {
            kapt_cp_jars.push(jar.clone());
        }
    }
    if !kapt_cp_jars.is_empty() {
        let cp = classpath_string_with_stdlib(&kapt_cp_jars, kotlin_home);
        cmd = cmd.arg("-classpath").arg(&cp);
    }

    let kapt_throwaway = generated_dir.join("kapt").join("kapt_classes");
    let _ = std::fs::create_dir_all(&kapt_throwaway);
    cmd = cmd
        .arg("-d")
        .arg(kapt_throwaway.to_string_lossy().to_string());

    let mut added = 0;
    for src in sources {
        if !references_generated_imports(src) {
            cmd = cmd.arg(src.to_string_lossy().to_string());
            added += 1;
        }
    }

    if added == 0 {
        return Ok(false);
    }

    let output = cmd.exec().map_err(|e| KargoError::Generic {
        message: format!("Failed to run KAPT pass: {e}"),
    })?;

    let stdout_text = String::from_utf8_lossy(&output.stdout);
    let stderr_text = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() {
        if !stdout_text.is_empty() {
            eprintln!("{stdout_text}");
        }
        if !stderr_text.is_empty() {
            eprintln!("{stderr_text}");
        }

        let has_real_errors =
            stderr_text.contains("e: ") && !stderr_text.contains("unresolved reference");
        if has_real_errors {
            return Err(KargoError::Generic {
                message: "KAPT annotation processing failed (see errors above)".into(),
            }
            .into());
        }
    }

    let _ = std::fs::remove_dir_all(&kapt_throwaway);
    let _ = std::fs::remove_dir_all(&stubs_dir);

    let generated = generated_sources.is_dir() && walkdir_has_java(&generated_sources);

    Ok(generated)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a classpath string that includes the Kotlin stdlib JARs.
fn classpath_string_with_stdlib(jars: &[PathBuf], kotlin_home: &Path) -> String {
    let kotlin_lib = kotlin_home.join("lib");
    let mut all: Vec<PathBuf> = jars.to_vec();
    for name in &[
        "kotlin-stdlib.jar",
        "annotations-13.0.jar",
        "kotlin-annotations-jvm.jar",
    ] {
        let jar = kotlin_lib.join(name);
        if jar.is_file() && !all.iter().any(|p| p.ends_with(name)) {
            all.push(jar);
        }
    }
    to_classpath_string(&all)
}

/// Discover annotation processor classes from JAR service files.
fn discover_processor_classes(jars: &[PathBuf]) -> Vec<String> {
    let mut classes = Vec::new();
    for jar in jars {
        let Ok(file) = std::fs::File::open(jar) else {
            continue;
        };
        let Ok(mut archive) = zip::ZipArchive::new(file) else {
            continue;
        };
        let Ok(mut entry) =
            archive.by_name("META-INF/services/javax.annotation.processing.Processor")
        else {
            continue;
        };
        let mut buf = String::new();
        if std::io::Read::read_to_string(&mut entry, &mut buf).is_ok() {
            for line in buf.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('#') {
                    classes.push(trimmed.to_string());
                }
            }
        }
    }
    classes
}

/// Quick check if a source file imports code generated by annotation processors
/// (KSP or KAPT). These files are excluded from the annotation processing pass
/// to avoid "unresolved reference" errors for classes that don't exist yet.
pub fn references_generated_imports(path: &Path) -> bool {
    if let Ok(content) = std::fs::read_to_string(path) {
        for line in content.lines().take(40) {
            let trimmed = line.trim();
            if !trimmed.starts_with("import ") {
                continue;
            }
            if trimmed.contains(".ksp.generated") {
                return true;
            }
            if trimmed.contains(".generated.") {
                return true;
            }
            if let Some(class_name) = trimmed.trim_end_matches(';').rsplit('.').next() {
                if class_name.starts_with("Dagger") {
                    return true;
                }
            }
        }
    }
    false
}

pub fn walkdir_has_java(dir: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if walkdir_has_java(&p) {
                return true;
            }
        } else if p.extension().is_some_and(|ext| ext == "java") {
            return true;
        }
    }
    false
}
