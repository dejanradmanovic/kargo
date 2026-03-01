//! Classpath assembly from the lockfile and local Maven cache.
//!
//! Separates compile-scoped JARs from test-scoped JARs so that main
//! sources compile against only their declared dependencies and tests
//! get the full classpath.

use std::path::{Path, PathBuf};

use kargo_core::lockfile::Lockfile;
use kargo_maven::cache::LocalCache;

/// Assembled classpath for a build.
#[derive(Debug, Clone)]
pub struct Classpath {
    /// JARs required for compiling main sources.
    pub compile_jars: Vec<PathBuf>,
    /// Additional JARs required for compiling test sources (includes compile_jars).
    pub test_jars: Vec<PathBuf>,
    /// JARs for annotation processors (KSP/KAPT) — only needed at build time,
    /// never included in runtime classpath or output JAR.
    pub processor_jars: Vec<PathBuf>,
}

/// Build the classpath from the lockfile and local cache.
///
/// Compile-scoped JARs are those with `scope == "compile"` (or no scope).
/// Test-scoped JARs are those with `scope == "test"`.
/// Processor-scoped JARs (`ksp`, `kapt`) are excluded from both — they are
/// only needed during annotation processing which fetches them separately.
/// The `test_jars` vector contains compile + test JARs.
pub fn assemble(project_root: &Path, lockfile: &Lockfile) -> Classpath {
    let cache = LocalCache::new(project_root);
    let mut compile_jars = Vec::new();
    let mut test_only_jars = Vec::new();
    let mut processor_jars = Vec::new();

    for pkg in &lockfile.package {
        let jar_path = match cache.get_jar(&pkg.group, &pkg.name, &pkg.version, None) {
            Some(p) => p,
            None => continue,
        };

        let scope = pkg.scope.as_deref().unwrap_or("compile");

        match scope {
            "test" => test_only_jars.push(jar_path),
            "ksp" | "kapt" => processor_jars.push(jar_path),
            _ => compile_jars.push(jar_path),
        }
    }

    compile_jars.sort();
    test_only_jars.sort();
    processor_jars.sort();

    let mut test_jars = compile_jars.clone();
    test_jars.extend(test_only_jars);

    Classpath {
        compile_jars,
        test_jars,
        processor_jars,
    }
}

/// Standard Kotlin stdlib JARs needed for compilation.
pub const STDLIB_JARS: &[&str] = &[
    "kotlin-stdlib.jar",
    "annotations-13.0.jar",
    "kotlin-annotations-jvm.jar",
];

/// Extended stdlib JARs list including JDK-specific variants (for runtime).
pub const STDLIB_RUNTIME_JARS: &[&str] = &[
    "kotlin-stdlib.jar",
    "kotlin-stdlib-jdk8.jar",
    "kotlin-stdlib-jdk7.jar",
];

/// Build a classpath string that includes the Kotlin stdlib JARs.
///
/// Appends the standard stdlib JARs from `kotlin_home/lib/` to the given JARs,
/// deduplicating by filename.
pub fn classpath_string_with_stdlib(jars: &[PathBuf], kotlin_home: &Path) -> String {
    let kotlin_lib = kotlin_home.join("lib");
    let mut all: Vec<PathBuf> = jars.to_vec();
    for name in STDLIB_JARS {
        let jar = kotlin_lib.join(name);
        if jar.is_file() && !all.iter().any(|p| p.ends_with(name)) {
            all.push(jar);
        }
    }
    to_classpath_string(&all)
}

/// Join JAR paths into a classpath string suitable for `-classpath`.
pub fn to_classpath_string(jars: &[PathBuf]) -> String {
    jars.iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(classpath_separator())
}

fn classpath_separator() -> &'static str {
    if cfg!(windows) {
        ";"
    } else {
        ":"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classpath_string_format() {
        let jars = vec![PathBuf::from("/a/b.jar"), PathBuf::from("/c/d.jar")];
        let s = to_classpath_string(&jars);
        assert!(s.contains("/a/b.jar"));
        assert!(s.contains("/c/d.jar"));
    }
}
