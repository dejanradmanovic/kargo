use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Output};

use crate::errors::KargoError;

/// Builder for constructing and executing external processes.
///
/// Provides a fluent API for setting program, arguments, environment variables, and working directory.
pub struct CommandBuilder {
    program: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    cwd: Option<String>,
}

impl CommandBuilder {
    /// Create a new builder for the given program.
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: HashMap::new(),
            cwd: None,
        }
    }

    /// Append a single argument.
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Append multiple arguments.
    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// Set an environment variable for the child process.
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Set the working directory for the child process.
    pub fn cwd(mut self, dir: impl Into<String>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    /// Execute the command and return its output.
    pub fn exec(&self) -> Result<Output, KargoError> {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        if let Some(ref dir) = self.cwd {
            cmd.current_dir(Path::new(dir));
        }
        cmd.output().map_err(KargoError::from)
    }
}
