//! Native Kotlin compiler (kotlinc-native) invocation and CInterop.
//!
//! Stub implementation that returns "not yet supported" errors.
//! Full implementation planned for Phase 5 (KMP).

use std::path::PathBuf;

use kargo_core::target::KotlinTarget;
use kargo_toolchain::discovery::ToolchainPaths;

use crate::dispatch::TargetCompiler;
use crate::env::BuildEnv;
use crate::unit::{CompilationOutput, CompilationUnit};

pub struct NativeCompiler {
    target: KotlinTarget,
}

impl NativeCompiler {
    pub fn new(target: KotlinTarget) -> Self {
        Self { target }
    }
}

impl TargetCompiler for NativeCompiler {
    fn compile(
        &self,
        _unit: &CompilationUnit,
        _env: &BuildEnv,
    ) -> miette::Result<CompilationOutput> {
        Err(kargo_util::errors::KargoError::Generic {
            message: format!(
                "Compilation for target {} is not yet supported. JVM builds are available.\n  \
                 Native target support is planned for Phase 5 (KMP).",
                self.target
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
                self.target
            ),
        }
        .into())
    }

    fn target(&self) -> KotlinTarget {
        self.target
    }

    fn compiler_binary(&self, toolchain: &ToolchainPaths) -> PathBuf {
        toolchain
            .kotlin_native
            .clone()
            .unwrap_or_else(|| toolchain.home.join("bin").join("kotlinc-native"))
    }
}
