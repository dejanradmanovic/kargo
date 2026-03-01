//! JVM Kotlin compiler (kotlinc) invocation and argument building.
//!
//! Implements [`TargetCompiler`] for JVM and Android JVM targets.

use std::path::PathBuf;

use kargo_core::target::KotlinTarget;
use kargo_toolchain::discovery::ToolchainPaths;
use kargo_util::errors::KargoError;

use crate::classpath::to_classpath_string;
use crate::dispatch::TargetCompiler;
use crate::env::BuildEnv;
use crate::unit::{CompilationOutput, CompilationUnit, Diagnostic, DiagnosticSeverity};

/// Compiler backend for JVM and Android JVM targets.
pub struct JvmCompiler {
    target: KotlinTarget,
    toolchain: ToolchainPaths,
    jdk_home: PathBuf,
    java_target: String,
}

impl JvmCompiler {
    pub fn new(
        target: KotlinTarget,
        toolchain: ToolchainPaths,
        jdk_home: PathBuf,
        java_target: String,
    ) -> Self {
        Self {
            target,
            toolchain,
            jdk_home,
            java_target,
        }
    }

    fn invoke(
        &self,
        unit: &CompilationUnit,
        env: &BuildEnv,
        output_dir: &PathBuf,
    ) -> miette::Result<CompilationOutput> {
        let all_sources = unit.all_sources();
        if all_sources.is_empty() {
            return Ok(CompilationOutput {
                classes_dir: output_dir.clone(),
                success: true,
                diagnostics: vec![],
            });
        }

        std::fs::create_dir_all(output_dir).map_err(KargoError::Io)?;

        let mut args: Vec<String> = vec![
            "-d".into(),
            output_dir.to_string_lossy().into(),
            "-jvm-target".into(),
            self.java_target.clone(),
        ];

        // Classpath
        if !unit.classpath.is_empty() {
            args.push("-classpath".into());
            args.push(to_classpath_string(&unit.classpath));
        }

        // User-specified compiler args from the profile
        args.extend(unit.compiler_args.iter().cloned());

        // Source files
        for src in &all_sources {
            args.push(src.to_string_lossy().into());
        }

        let kotlinc_bin = self.compiler_binary(&self.toolchain);

        let mut cmd =
            kargo_util::process::CommandBuilder::new(kotlinc_bin.to_string_lossy().to_string());
        cmd = cmd
            .args(args)
            .env("JAVA_HOME", self.jdk_home.to_string_lossy().to_string());

        for (k, v) in &env.vars {
            cmd = cmd.env(k, v);
        }

        let output = cmd.exec().map_err(|e| KargoError::Generic {
            message: format!("Failed to execute kotlinc: {e}"),
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut diagnostics = parse_diagnostics(&stdout, &stderr);

        if !output.status.success() && diagnostics.is_empty() {
            let raw = format!("{stdout}\n{stderr}").trim().to_string();
            if !raw.is_empty() {
                diagnostics.push(Diagnostic {
                    severity: DiagnosticSeverity::Error,
                    message: raw,
                    file: None,
                    line: None,
                });
            }
        }

        Ok(CompilationOutput {
            classes_dir: output_dir.clone(),
            success: output.status.success(),
            diagnostics,
        })
    }
}

impl TargetCompiler for JvmCompiler {
    fn compile(&self, unit: &CompilationUnit, env: &BuildEnv) -> miette::Result<CompilationOutput> {
        self.invoke(unit, env, &unit.output_dir)
    }

    fn check_only(
        &self,
        unit: &CompilationUnit,
        env: &BuildEnv,
    ) -> miette::Result<CompilationOutput> {
        let tmp = tempfile::tempdir().map_err(KargoError::Io)?;
        let tmp_out = tmp.path().to_path_buf();
        self.invoke(unit, env, &tmp_out)
    }

    fn target(&self) -> KotlinTarget {
        self.target
    }

    fn compiler_binary(&self, toolchain: &ToolchainPaths) -> PathBuf {
        toolchain.kotlinc.clone()
    }
}

fn parse_diagnostics(stdout: &str, stderr: &str) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let combined = format!("{stdout}\n{stderr}");

    for line in combined.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.contains(": error:") {
            diagnostics.push(parse_diagnostic_line(trimmed, DiagnosticSeverity::Error));
        } else if trimmed.contains(": warning:") {
            diagnostics.push(parse_diagnostic_line(trimmed, DiagnosticSeverity::Warning));
        } else if trimmed.contains(": info:") {
            diagnostics.push(parse_diagnostic_line(trimmed, DiagnosticSeverity::Info));
        }
    }

    diagnostics
}

fn parse_diagnostic_line(line: &str, severity: DiagnosticSeverity) -> Diagnostic {
    // kotlinc format: "file.kt:line:col: severity: message"
    let parts: Vec<&str> = line
        .splitn(
            2,
            match severity {
                DiagnosticSeverity::Error => ": error:",
                DiagnosticSeverity::Warning => ": warning:",
                DiagnosticSeverity::Info => ": info:",
            },
        )
        .collect();

    let (file, line_num) = if let Some(location) = parts.first() {
        let loc_parts: Vec<&str> = location.rsplitn(3, ':').collect();
        if loc_parts.len() >= 2 {
            let line_num = loc_parts.first().and_then(|s| s.parse::<u32>().ok());
            let file = if loc_parts.len() >= 3 {
                // Reconstruct file path (may contain colons on Windows)
                Some(
                    loc_parts[2..]
                        .iter()
                        .rev()
                        .copied()
                        .collect::<Vec<_>>()
                        .join(":"),
                )
            } else {
                Some(loc_parts.last().unwrap_or(&"").to_string())
            };
            (file, line_num)
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    let message = parts.get(1).unwrap_or(&line).trim().to_string();

    Diagnostic {
        severity,
        message,
        file,
        line: line_num,
    }
}
