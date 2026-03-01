//! KSP (Kotlin Symbol Processing) toolchain management and execution.

use std::path::{Path, PathBuf};

use kargo_core::manifest::Manifest;
use kargo_maven::cache::LocalCache;
use kargo_util::errors::KargoError;

use super::{ensure_maven_jar, ProcessorInfo, ProcessorKind};
use crate::classpath::to_classpath_string;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const KSP_GROUP: &str = "com.google.devtools.ksp";
const KSP_API_ARTIFACT: &str = "symbol-processing-api";
const KSP_CMDLINE_ARTIFACT: &str = "symbol-processing-cmdline";
const KSP_AA_ARTIFACT: &str = "symbol-processing-aa";
const KSP_COMMON_DEPS_ARTIFACT: &str = "symbol-processing-common-deps";

const INTELLIJ_COROUTINES_GROUP: &str = "org.jetbrains.intellij.deps.kotlinx";
const INTELLIJ_COROUTINES_ARTIFACT: &str = "kotlinx-coroutines-core-jvm";

/// Default IntelliJ coroutines version when POM lookup fails.
const INTELLIJ_COROUTINES_DEFAULT_VERSION: &str = "1.8.0-intellij-14";

const KSP_PLUGIN_ID: &str = "com.google.devtools.ksp.symbol-processing";

// ---------------------------------------------------------------------------
// KSP version detection and resolution
// ---------------------------------------------------------------------------

/// Return `(group, artifact, version)` tuples for all KSP toolchain JARs that
/// are auto-provisioned into the project cache but not tracked by the resolver.
///
/// Call this with the manifest's `ksp-version` to protect these entries from
/// being pruned by `cache.prune()`.
pub fn auto_provisioned_ksp_jars(
    ksp_version: &str,
    cache: &LocalCache,
) -> Vec<(String, String, String)> {
    let mut coords = Vec::new();
    if ksp_version.is_empty() {
        return coords;
    }

    if is_ksp2(ksp_version) {
        coords.push((
            KSP_GROUP.into(),
            KSP_AA_ARTIFACT.into(),
            ksp_version.into(),
        ));
        coords.push((
            KSP_GROUP.into(),
            KSP_API_ARTIFACT.into(),
            ksp_version.into(),
        ));
        coords.push((
            KSP_GROUP.into(),
            KSP_COMMON_DEPS_ARTIFACT.into(),
            ksp_version.into(),
        ));

        let pom_path = cache.artifact_dir(KSP_GROUP, KSP_AA_ARTIFACT, ksp_version);
        let pom_file = pom_path.join(format!("{KSP_AA_ARTIFACT}-{ksp_version}.pom"));
        let coroutines_ver = std::fs::read_to_string(&pom_file)
            .ok()
            .and_then(|content| {
                extract_pom_dep_version(
                    &content,
                    INTELLIJ_COROUTINES_GROUP,
                    INTELLIJ_COROUTINES_ARTIFACT,
                )
            })
            .unwrap_or_else(|| INTELLIJ_COROUTINES_DEFAULT_VERSION.to_string());

        coords.push((
            INTELLIJ_COROUTINES_GROUP.into(),
            INTELLIJ_COROUTINES_ARTIFACT.into(),
            coroutines_ver,
        ));
    } else {
        coords.push((
            KSP_GROUP.into(),
            KSP_CMDLINE_ARTIFACT.into(),
            ksp_version.into(),
        ));
        coords.push((
            KSP_GROUP.into(),
            KSP_API_ARTIFACT.into(),
            ksp_version.into(),
        ));
    }

    coords
}

/// Returns `true` if the KSP version uses the KSP2 standalone format (>= 2.3.0).
///
/// KSP1 versions contain a `-` separator (e.g. `2.2.21-2.0.5` = kotlinVer-kspVer).
/// KSP2 versions are standalone (e.g. `2.3.0`, `2.3.6`).
pub fn is_ksp2(version: &str) -> bool {
    if version.contains('-') {
        return false;
    }
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() >= 2 {
        let major: u32 = parts[0].parse().unwrap_or(0);
        let minor: u32 = parts[1].parse().unwrap_or(0);
        return major > 2 || (major == 2 && minor >= 3);
    }
    false
}

/// Determine the KSP version to use.
///
/// Priority: explicit `ksp-version` in `[package]` > derived from Kotlin version.
pub fn resolve_ksp_version(manifest: &Manifest) -> String {
    if let Some(ref v) = manifest.package.ksp_version {
        return v.clone();
    }
    manifest.package.kotlin.clone()
}

// ---------------------------------------------------------------------------
// KSP artifact resolution
// ---------------------------------------------------------------------------

/// Resolved KSP tooling JARs ready for invocation.
pub enum KspToolchain {
    /// KSP1: compiler plugin JARs used with `-Xplugin` in `kotlinc`.
    Ksp1 {
        cmdline_jar: PathBuf,
        api_jar: PathBuf,
    },
    /// KSP2: standalone JARs invoked via `java -cp ... KSPJvmMain`.
    Ksp2 {
        aa_jar: PathBuf,
        api_jar: PathBuf,
        common_deps_jar: PathBuf,
        coroutines_jar: PathBuf,
    },
}

/// Ensure KSP toolchain JARs are available in the local cache.
pub async fn ensure_ksp_toolchain(
    cache: &LocalCache,
    ksp_version: &str,
) -> miette::Result<Option<KspToolchain>> {
    if is_ksp2(ksp_version) {
        ensure_ksp2_toolchain(cache, ksp_version).await
    } else {
        ensure_ksp1_toolchain(cache, ksp_version).await
    }
}

async fn ensure_ksp1_toolchain(
    cache: &LocalCache,
    ksp_version: &str,
) -> miette::Result<Option<KspToolchain>> {
    let cmdline = ensure_maven_jar(cache, KSP_GROUP, KSP_CMDLINE_ARTIFACT, ksp_version).await?;
    let api = ensure_maven_jar(cache, KSP_GROUP, KSP_API_ARTIFACT, ksp_version).await?;
    match (cmdline, api) {
        (Some(c), Some(a)) => Ok(Some(KspToolchain::Ksp1 {
            cmdline_jar: c,
            api_jar: a,
        })),
        _ => Ok(None),
    }
}

async fn ensure_ksp2_toolchain(
    cache: &LocalCache,
    ksp_version: &str,
) -> miette::Result<Option<KspToolchain>> {
    let github_jars = ensure_ksp2_from_github(cache, ksp_version).await?;
    let (aa_jar, api_jar, common_deps_jar) = match github_jars {
        Some(jars) => jars,
        None => return Ok(None),
    };

    let coroutines_version = resolve_ksp2_coroutines_version(cache, ksp_version).await;
    let coroutines_jar = ensure_maven_jar(
        cache,
        INTELLIJ_COROUTINES_GROUP,
        INTELLIJ_COROUTINES_ARTIFACT,
        &coroutines_version,
    )
    .await?;

    match coroutines_jar {
        Some(cj) => Ok(Some(KspToolchain::Ksp2 {
            aa_jar,
            api_jar,
            common_deps_jar,
            coroutines_jar: cj,
        })),
        None => {
            eprintln!(
                "  Warning: IntelliJ coroutines {coroutines_version} not found. \
                 KSP2 may not work correctly."
            );
            Ok(None)
        }
    }
}

/// Determine the IntelliJ coroutines version required by a KSP2 release.
///
/// Parses the POM from the cached `symbol-processing-aa` artifact if available,
/// otherwise falls back to a known-good default.
async fn resolve_ksp2_coroutines_version(cache: &LocalCache, ksp_version: &str) -> String {
    let pom_path = cache.artifact_dir(KSP_GROUP, KSP_AA_ARTIFACT, ksp_version);
    let pom_file = pom_path.join(format!("{KSP_AA_ARTIFACT}-{ksp_version}.pom"));

    if let Ok(content) = std::fs::read_to_string(&pom_file) {
        if let Some(ver) = extract_pom_dep_version(
            &content,
            INTELLIJ_COROUTINES_GROUP,
            INTELLIJ_COROUTINES_ARTIFACT,
        ) {
            return ver;
        }
    }

    INTELLIJ_COROUTINES_DEFAULT_VERSION.to_string()
}

fn extract_pom_dep_version(pom_xml: &str, group: &str, artifact: &str) -> Option<String> {
    let group_tag = format!("<groupId>{group}</groupId>");
    let artifact_tag = format!("<artifactId>{artifact}</artifactId>");

    for chunk in pom_xml.split("<dependency>") {
        if chunk.contains(&group_tag) && chunk.contains(&artifact_tag) {
            if let Some(start) = chunk.find("<version>") {
                let rest = &chunk[start + 9..];
                if let Some(end) = rest.find("</version>") {
                    return Some(rest[..end].to_string());
                }
            }
        }
    }
    None
}

/// Download KSP 2.3.x+ JARs from GitHub Releases.
async fn ensure_ksp2_from_github(
    cache: &LocalCache,
    ksp_version: &str,
) -> miette::Result<Option<(PathBuf, PathBuf, PathBuf)>> {
    let aa_cached = cache.get_jar(KSP_GROUP, KSP_AA_ARTIFACT, ksp_version, None);
    let api_cached = cache.get_jar(KSP_GROUP, KSP_API_ARTIFACT, ksp_version, None);
    let deps_cached = cache.get_jar(KSP_GROUP, KSP_COMMON_DEPS_ARTIFACT, ksp_version, None);

    if let (Some(aa), Some(api), Some(deps)) = (aa_cached, api_cached, deps_cached) {
        return Ok(Some((aa, api, deps)));
    }

    eprintln!("  Downloading KSP {ksp_version} from GitHub...");

    let url =
        format!("https://github.com/google/ksp/releases/download/{ksp_version}/artifacts.zip");

    let client = kargo_maven::download::build_client()?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| KargoError::Generic {
            message: format!("Failed to download KSP artifacts: {e}"),
        })?;

    if !resp.status().is_success() {
        eprintln!(
            "  Warning: KSP {ksp_version} not found on GitHub (HTTP {})",
            resp.status()
        );
        return Ok(None);
    }

    let zip_bytes = resp.bytes().await.map_err(|e| KargoError::Generic {
        message: format!("Failed to read KSP archive: {e}"),
    })?;

    let reader = std::io::Cursor::new(&zip_bytes);
    let mut archive = zip::ZipArchive::new(reader).map_err(|e| KargoError::Generic {
        message: format!("Failed to open KSP zip: {e}"),
    })?;

    let targets = [
        (KSP_AA_ARTIFACT, ksp_version),
        (KSP_API_ARTIFACT, ksp_version),
        (KSP_COMMON_DEPS_ARTIFACT, ksp_version),
    ];
    let expected_paths: Vec<String> = targets
        .iter()
        .map(|(art, ver)| format!("com/google/devtools/ksp/{art}/{ver}/{art}-{ver}.jar"))
        .collect();

    let mut extracted: Vec<Option<Vec<u8>>> = vec![None; targets.len()];
    let mut pom_data: Option<Vec<u8>> = None;
    let aa_pom_path = format!(
        "com/google/devtools/ksp/{KSP_AA_ARTIFACT}/{ksp_version}/{KSP_AA_ARTIFACT}-{ksp_version}.pom"
    );

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| KargoError::Generic {
            message: format!("Failed to read zip entry: {e}"),
        })?;
        let name = file.name().to_string();

        for (idx, expected) in expected_paths.iter().enumerate() {
            if name == *expected {
                let mut buf = Vec::new();
                std::io::Read::read_to_end(&mut file, &mut buf).map_err(|e| {
                    KargoError::Generic {
                        message: format!("Failed to extract {}: {e}", targets[idx].0),
                    }
                })?;
                extracted[idx] = Some(buf);
                break;
            }
        }

        if name == aa_pom_path {
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut file, &mut buf);
            pom_data = Some(buf);
        }

        if extracted.iter().all(|e| e.is_some()) {
            break;
        }
    }

    let aa_data = extracted[0].take();
    let api_data = extracted[1].take();
    let deps_data = extracted[2].take();

    match (aa_data, api_data, deps_data) {
        (Some(aa), Some(api), Some(deps)) => {
            let aa_path = cache.put_jar(KSP_GROUP, KSP_AA_ARTIFACT, ksp_version, None, &aa)?;
            let api_path = cache.put_jar(KSP_GROUP, KSP_API_ARTIFACT, ksp_version, None, &api)?;
            let deps_path = cache.put_jar(
                KSP_GROUP,
                KSP_COMMON_DEPS_ARTIFACT,
                ksp_version,
                None,
                &deps,
            )?;

            if let Some(pom) = pom_data {
                let pom_dir = cache.artifact_dir(KSP_GROUP, KSP_AA_ARTIFACT, ksp_version);
                let _ = std::fs::create_dir_all(&pom_dir);
                let _ = std::fs::write(
                    pom_dir.join(format!("{KSP_AA_ARTIFACT}-{ksp_version}.pom")),
                    &pom,
                );
            }

            eprintln!("  KSP {ksp_version} installed");
            Ok(Some((aa_path, api_path, deps_path)))
        }
        _ => {
            eprintln!("  Warning: Required KSP JARs not found in release archive");
            Ok(None)
        }
    }
}

// ---------------------------------------------------------------------------
// KSP execution
// ---------------------------------------------------------------------------

/// Run KSP2 as a standalone pre-build step.
///
/// Invokes `java -cp <ksp-jars> com.google.devtools.ksp.cmdline.KSPJvmMain`
/// with the project sources and processor JARs. Generated `.kt` files are
/// written to `generated_dir/ksp/kotlin/`.
pub fn run_ksp2_standalone(
    ksp: &KspToolchain,
    processors: &[ProcessorInfo],
    cache: &LocalCache,
    source_dirs: &[PathBuf],
    library_jars: &[PathBuf],
    processor_scope_jars: &[PathBuf],
    kotlin_home: &Path,
    jdk_home: &Path,
    java_target: &str,
    project_dir: &Path,
    generated_dir: &Path,
    module_name: &str,
    ksp_options: &std::collections::BTreeMap<String, String>,
) -> miette::Result<bool> {
    let (aa_jar, api_jar, common_deps_jar, coroutines_jar) = match ksp {
        KspToolchain::Ksp2 {
            aa_jar,
            api_jar,
            common_deps_jar,
            coroutines_jar,
        } => (aa_jar, api_jar, common_deps_jar, coroutines_jar),
        _ => return Ok(false),
    };

    let ksp_procs: Vec<&ProcessorInfo> = processors
        .iter()
        .filter(|p| p.kind == ProcessorKind::Ksp)
        .collect();
    if ksp_procs.is_empty() {
        return Ok(false);
    }

    let proc_jars: Vec<PathBuf> = ksp_procs
        .iter()
        .filter_map(|p| cache.get_jar(&p.group, &p.artifact, &p.version, None))
        .collect();
    if proc_jars.is_empty() {
        return Ok(false);
    }

    let ksp_dir = generated_dir.join("ksp");
    let kotlin_out = ksp_dir.join("kotlin");
    let java_out = ksp_dir.join("java");
    let class_out = ksp_dir.join("classes");
    let resource_out = ksp_dir.join("resources");
    let caches_dir = ksp_dir.join("caches");
    for dir in [
        &kotlin_out,
        &java_out,
        &class_out,
        &resource_out,
        &caches_dir,
    ] {
        std::fs::create_dir_all(dir).map_err(KargoError::Io)?;
    }

    let stdlib_jar = kotlin_home.join("lib").join("kotlin-stdlib.jar");
    let tool_cp = to_classpath_string(&[
        aa_jar.clone(),
        api_jar.clone(),
        common_deps_jar.clone(),
        stdlib_jar,
        coroutines_jar.clone(),
    ]);

    let source_roots: Vec<String> = source_dirs
        .iter()
        .filter(|d| d.is_dir())
        .map(|d| d.to_string_lossy().to_string())
        .collect();
    if source_roots.is_empty() {
        return Ok(false);
    }
    let source_roots_str = source_roots.join(if cfg!(windows) { ";" } else { ":" });

    let libs_str = to_classpath_string(library_jars);

    let mut full_proc_cp = proc_jars.clone();
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
    let proc_cp = to_classpath_string(&full_proc_cp);

    let java_bin = jdk_home.join("bin").join("java");

    let mut cmd = kargo_util::process::CommandBuilder::new(java_bin.to_string_lossy().to_string())
        .arg("-cp")
        .arg(&tool_cp)
        .arg("com.google.devtools.ksp.cmdline.KSPJvmMain")
        .arg(format!("-jvm-target={java_target}"))
        .arg(format!("-module-name={module_name}"))
        .arg(format!("-source-roots={source_roots_str}"))
        .arg(format!(
            "-project-base-dir={}",
            project_dir.to_string_lossy()
        ))
        .arg(format!("-output-base-dir={}", ksp_dir.to_string_lossy()))
        .arg(format!("-caches-dir={}", caches_dir.to_string_lossy()))
        .arg(format!("-class-output-dir={}", class_out.to_string_lossy()))
        .arg(format!(
            "-kotlin-output-dir={}",
            kotlin_out.to_string_lossy()
        ))
        .arg(format!("-java-output-dir={}", java_out.to_string_lossy()))
        .arg(format!(
            "-resource-output-dir={}",
            resource_out.to_string_lossy()
        ))
        .arg("-language-version=2.0")
        .arg("-api-version=2.0")
        .arg("-incremental=false");

    if !ksp_options.is_empty() {
        let opts: Vec<String> = ksp_options
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        cmd = cmd.arg(format!("-processor-options={}", opts.join(":")));
    }

    if !libs_str.is_empty() {
        cmd = cmd.arg(format!("-libraries={libs_str}"));
    }

    cmd = cmd.arg(&proc_cp);

    cmd = cmd.env("JAVA_HOME", jdk_home.to_string_lossy().to_string());

    let output = cmd.exec().map_err(|e| KargoError::Generic {
        message: format!("Failed to run KSP2: {e}"),
    })?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stderr.lines().chain(stdout.lines()) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.contains("Worker exited due to exception")
            || trimmed.contains("java.lang.AssertionError")
        {
            continue;
        }
        if trimmed.starts_with("w:") || trimmed.starts_with("e:") {
            eprintln!("  {trimmed}");
        }
    }

    if !output.status.success() {
        return Err(KargoError::Generic {
            message: format!(
                "KSP2 annotation processing failed (exit code {}):\n{}",
                output.status.code().unwrap_or(-1),
                stderr
            ),
        }
        .into());
    }

    let has_generated = kotlin_out.is_dir()
        && std::fs::read_dir(&kotlin_out)
            .map(|rd| rd.flatten().next().is_some())
            .unwrap_or(false);

    Ok(has_generated)
}

/// Build KSP1 compiler arguments for a KSP-only pass with `kotlinc`.
///
/// Returns the extra compiler args to inject into a `kotlinc` invocation
/// that will run KSP as a compiler plugin. The compilation output from
/// this pass is discarded; only the KSP-generated files matter.
pub fn build_ksp1_args(
    processors: &[ProcessorInfo],
    cache: &LocalCache,
    ksp_toolchain: &KspToolchain,
    processor_scope_jars: &[PathBuf],
    generated_dir: &Path,
    project_dir: &Path,
    ksp_options: &std::collections::BTreeMap<String, String>,
) -> Vec<String> {
    let (cmdline_jar, api_jar) = match ksp_toolchain {
        KspToolchain::Ksp1 {
            cmdline_jar,
            api_jar,
        } => (cmdline_jar, api_jar),
        _ => return vec![],
    };

    let ksp_procs: Vec<&ProcessorInfo> = processors
        .iter()
        .filter(|p| p.kind == ProcessorKind::Ksp)
        .collect();
    if ksp_procs.is_empty() {
        return vec![];
    }

    let proc_jars: Vec<PathBuf> = ksp_procs
        .iter()
        .filter_map(|p| cache.get_jar(&p.group, &p.artifact, &p.version, None))
        .collect();
    if proc_jars.is_empty() {
        return vec![];
    }

    let mut full_proc_jars = proc_jars;
    for jar in processor_scope_jars {
        if !full_proc_jars.contains(jar) {
            full_proc_jars.push(jar.clone());
        }
    }
    let proc_classpath = to_classpath_string(&full_proc_jars);

    let ksp_dir = generated_dir.join("ksp");
    let kotlin_out = ksp_dir.join("kotlin");
    let java_out = ksp_dir.join("java");
    let class_out = ksp_dir.join("classes");
    let resource_out = ksp_dir.join("resources");
    let caches_dir = ksp_dir.join("caches");
    for dir in [
        &kotlin_out,
        &java_out,
        &class_out,
        &resource_out,
        &caches_dir,
    ] {
        let _ = std::fs::create_dir_all(dir);
    }

    let mut args = Vec::new();

    args.push(format!("-Xplugin={}", cmdline_jar.to_string_lossy()));
    args.push(format!("-Xplugin={}", api_jar.to_string_lossy()));
    args.push("-Xallow-no-source-files".to_string());

    args.push(format!(
        "-P=plugin:{KSP_PLUGIN_ID}:apclasspath={proc_classpath}"
    ));
    args.push(format!(
        "-P=plugin:{KSP_PLUGIN_ID}:projectBaseDir={}",
        project_dir.to_string_lossy()
    ));
    args.push(format!(
        "-P=plugin:{KSP_PLUGIN_ID}:kotlinOutputDir={}",
        kotlin_out.to_string_lossy()
    ));
    args.push(format!(
        "-P=plugin:{KSP_PLUGIN_ID}:javaOutputDir={}",
        java_out.to_string_lossy()
    ));
    args.push(format!(
        "-P=plugin:{KSP_PLUGIN_ID}:classOutputDir={}",
        class_out.to_string_lossy()
    ));
    args.push(format!(
        "-P=plugin:{KSP_PLUGIN_ID}:resourceOutputDir={}",
        resource_out.to_string_lossy()
    ));
    args.push(format!(
        "-P=plugin:{KSP_PLUGIN_ID}:kspOutputDir={}",
        ksp_dir.to_string_lossy()
    ));
    args.push(format!(
        "-P=plugin:{KSP_PLUGIN_ID}:cachesDir={}",
        caches_dir.to_string_lossy()
    ));
    args.push(format!("-P=plugin:{KSP_PLUGIN_ID}:incremental=false"));

    for (key, value) in ksp_options {
        args.push(format!("-P=plugin:{KSP_PLUGIN_ID}:apoption={key}={value}"));
    }

    args
}
