//! Target-agnostic compiler dispatch.
//!
//! Defines the [`TargetCompiler`] trait that all backend compilers implement,
//! and [`CompilerDispatch`] which resolves the correct backend for a given
//! [`KotlinTarget`]. Adding a new target backend requires implementing
//! `TargetCompiler` and registering it here — zero changes to the build
//! orchestration layer.

use std::path::PathBuf;

use kargo_core::target::KotlinTarget;
use kargo_toolchain::discovery::ToolchainPaths;

use crate::env::BuildEnv;
use crate::unit::{CompilationOutput, CompilationUnit};

/// Trait implemented by each target-specific compiler backend.
pub trait TargetCompiler {
    /// Compile the given unit and produce output artifacts.
    fn compile(&self, unit: &CompilationUnit, env: &BuildEnv) -> miette::Result<CompilationOutput>;

    /// Type-check without producing permanent output artifacts.
    fn check_only(
        &self,
        unit: &CompilationUnit,
        env: &BuildEnv,
    ) -> miette::Result<CompilationOutput>;

    /// The target this compiler handles.
    fn target(&self) -> KotlinTarget;

    /// Path to the compiler binary for this target.
    fn compiler_binary(&self, toolchain: &ToolchainPaths) -> PathBuf;
}

/// Resolves the correct [`TargetCompiler`] for a given target.
pub struct CompilerDispatch;

impl CompilerDispatch {
    /// Get a compiler backend for the given target.
    pub fn resolve(
        target: KotlinTarget,
        toolchain: ToolchainPaths,
        jdk_home: PathBuf,
        java_target: String,
    ) -> Box<dyn TargetCompiler> {
        match target {
            KotlinTarget::Jvm | KotlinTarget::Android => {
                let c = crate::kotlinc::JvmCompiler::new(target, toolchain, jdk_home, java_target);
                Box::new(c)
            }
            KotlinTarget::Js => Box::new(crate::kotlinc_js::JsCompiler::new(target)),
            _ if target.is_native() => Box::new(crate::kotlinc_native::NativeCompiler::new(target)),
            _ => Box::new(UnsupportedCompiler(target)),
        }
    }
}

/// Placeholder for targets with no backend yet (WASM without native flag, etc.).
struct UnsupportedCompiler(KotlinTarget);

impl TargetCompiler for UnsupportedCompiler {
    fn compile(
        &self,
        _unit: &CompilationUnit,
        _env: &BuildEnv,
    ) -> miette::Result<CompilationOutput> {
        Err(kargo_util::errors::KargoError::Generic {
            message: format!(
                "Compilation for target {} is not yet supported. JVM builds are available.",
                self.0
            ),
        }
        .into())
    }

    fn check_only(
        &self,
        _unit: &CompilationUnit,
        _env: &BuildEnv,
    ) -> miette::Result<CompilationOutput> {
        Err(kargo_util::errors::KargoError::Generic {
            message: format!(
                "Type-checking for target {} is not yet supported. JVM builds are available.",
                self.0
            ),
        }
        .into())
    }

    fn target(&self) -> KotlinTarget {
        self.0
    }

    fn compiler_binary(&self, _toolchain: &ToolchainPaths) -> PathBuf {
        PathBuf::from("unsupported")
    }
}
