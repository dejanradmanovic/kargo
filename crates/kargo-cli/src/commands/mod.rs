//! Command dispatch and handler modules.

mod build;
mod clean;
mod env;
mod init;
mod new;

use miette::Result;

use crate::cli::{Cli, Command};

/// Route a parsed CLI invocation to the appropriate command handler.
pub fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Command::New { name, template } => new::exec(&name, &template),
        Command::Init { template } => init::exec(&template),
        Command::Clean { variant } => clean::exec(variant.as_deref()),
        Command::Env { reveal } => env::exec(reveal),
        Command::Build { .. } => {
            eprintln!("kargo build is not yet implemented");
            Ok(())
        }
        _ => {
            eprintln!("This command is not yet implemented");
            Ok(())
        }
    }
}
