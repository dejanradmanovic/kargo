//! Kotlin compiler plugin management: KSP and KAPT annotation processing.
//!
//! Supports two KSP modes:
//!
//! - **KSP1** (versions like `2.2.21-2.0.5`): Runs as a `kotlinc` compiler
//!   plugin via `-Xplugin`. The `symbol-processing-cmdline` JAR is fetched
//!   from Maven Central and loaded as part of a KSP-only compilation pass
//!   that generates sources before the main build.
//!
//! - **KSP2** (versions `2.3.0`+): Runs as a standalone pre-build step via
//!   `java -cp ... KSPJvmMain`. The `symbol-processing-aa` and runtime
//!   dependencies are fetched from GitHub Releases and Maven Central.
//!   KSP2 processes sources and outputs generated `.kt` files that the
//!   main `kotlinc` compilation then picks up.

pub mod kapt;
pub mod ksp;

pub use kapt::*;
pub use ksp::*;

use std::path::PathBuf;

use kargo_core::dependency::Dependency;
use kargo_core::manifest::Manifest;
use kargo_maven::cache::LocalCache;

// ---------------------------------------------------------------------------
// Processor detection
// ---------------------------------------------------------------------------

/// A detected annotation processor.
#[derive(Debug, Clone)]
pub struct ProcessorInfo {
    pub name: String,
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub kind: ProcessorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessorKind {
    Ksp,
    Kapt,
}

/// Detect KSP and KAPT processors from the manifest.
pub fn detect_processors(manifest: &Manifest, _cache: &LocalCache) -> Vec<ProcessorInfo> {
    let mut processors = Vec::new();

    for (name, dep) in &manifest.ksp {
        if let Some(info) = parse_processor(name, dep, ProcessorKind::Ksp) {
            processors.push(info);
        }
    }

    for (name, dep) in &manifest.kapt {
        if let Some(info) = parse_processor(name, dep, ProcessorKind::Kapt) {
            processors.push(info);
        }
    }

    processors
}

fn parse_processor(name: &str, dep: &Dependency, kind: ProcessorKind) -> Option<ProcessorInfo> {
    match dep {
        Dependency::Short(coord) => {
            let parts: Vec<&str> = coord.split(':').collect();
            if parts.len() < 3 {
                return None;
            }
            Some(ProcessorInfo {
                name: name.to_string(),
                group: parts[0].to_string(),
                artifact: parts[1].to_string(),
                version: parts[2].to_string(),
                kind,
            })
        }
        Dependency::Detailed(d) => Some(ProcessorInfo {
            name: name.to_string(),
            group: d.group.clone(),
            artifact: d.artifact.clone(),
            version: d.version.clone(),
            kind,
        }),
        Dependency::Catalog(_) => None,
    }
}

// ---------------------------------------------------------------------------
// Shared utilities
// ---------------------------------------------------------------------------

/// Download a single Maven JAR if not already cached.
pub async fn ensure_maven_jar(
    cache: &LocalCache,
    group: &str,
    artifact: &str,
    version: &str,
) -> miette::Result<Option<PathBuf>> {
    if let Some(path) = cache.get_jar(group, artifact, version, None) {
        return Ok(Some(path));
    }

    let repo = kargo_maven::repository::MavenRepository::maven_central();
    let client = kargo_maven::download::build_client()?;
    let url = repo.jar_url(group, artifact, version, None);
    let label = format!("{artifact}:{version}");

    match kargo_maven::download::download_artifact(&client, &repo, &url, &label).await? {
        Some(data) => {
            let path = cache.put_jar(group, artifact, version, None, &data)?;
            Ok(Some(path))
        }
        None => {
            eprintln!("  Warning: JAR not found: {group}:{artifact}:{version}");
            Ok(None)
        }
    }
}

/// Fetch processor JARs from Maven Central if not already cached.
pub async fn ensure_processor_jars(
    processors: &[ProcessorInfo],
    cache: &LocalCache,
) -> miette::Result<()> {
    for proc in processors {
        if cache
            .get_jar(&proc.group, &proc.artifact, &proc.version, None)
            .is_some()
        {
            continue;
        }
        ensure_maven_jar(cache, &proc.group, &proc.artifact, &proc.version).await?;
    }
    Ok(())
}
