//! Compilation unit: one compiler invocation with sources, classpath, and options.

use std::path::PathBuf;

use kargo_core::target::KotlinTarget;

/// A single compilation unit representing one `kotlinc` invocation.
#[derive(Debug, Clone)]
pub struct CompilationUnit {
    /// Human-readable name (e.g. "main", "test", "ksp").
    pub name: String,
    /// The Kotlin target this unit compiles for.
    pub target: KotlinTarget,
    /// Kotlin source files to compile.
    pub sources: Vec<PathBuf>,
    /// Resource directories to copy to the output.
    pub resource_dirs: Vec<PathBuf>,
    /// Dependency JAR files on the classpath.
    pub classpath: Vec<PathBuf>,
    /// Directory for compiled output (.class files, etc.).
    pub output_dir: PathBuf,
    /// Extra compiler arguments from the profile or user config.
    pub compiler_args: Vec<String>,
    /// Whether this unit compiles test sources.
    pub is_test: bool,
    /// Directories containing generated sources (KSP/KAPT/BuildConfig).
    pub generated_sources: Vec<PathBuf>,
    /// Annotation processor JAR paths (KSP/KAPT) — included in fingerprint
    /// so that changing a processor version triggers recompilation.
    pub processor_jars: Vec<PathBuf>,
}

impl CompilationUnit {
    /// Returns `true` if this unit has any sources to compile.
    pub fn has_sources(&self) -> bool {
        !self.sources.is_empty() || self.generated_sources.iter().any(|d| d.is_dir())
    }

    /// All source files: user sources + generated sources.
    pub fn all_sources(&self) -> Vec<PathBuf> {
        let mut all = self.sources.clone();
        for dir in &self.generated_sources {
            if dir.is_dir() {
                crate::source_set_discovery::collect_files_recursive_pub(dir, &mut all);
            }
        }
        all
    }
}

/// The result of compiling a single unit.
#[derive(Debug)]
pub struct CompilationOutput {
    /// Directory containing compiled artifacts.
    pub classes_dir: PathBuf,
    /// Whether compilation succeeded.
    pub success: bool,
    /// Compiler diagnostic messages (errors, warnings).
    pub diagnostics: Vec<Diagnostic>,
}

/// A single compiler diagnostic message.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub file: Option<String>,
    pub line: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}
