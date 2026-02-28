//! Command dispatch and handler modules.

mod add;
mod audit;
mod build;
mod clean;
mod env;
mod fetch;
mod init;
mod lock;
mod new;
mod outdated;
mod remove;
mod self_;
mod toolchain;
mod tree;
mod update;

use miette::Result;

use crate::cli::{Cli, Command};

/// Route a parsed CLI invocation to the appropriate command handler.
pub fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Command::New { name, template } => new::exec(&name, &template),
        Command::Init { template } => init::exec(&template),
        Command::Clean { variant } => clean::exec(variant.as_deref()),
        Command::Env { reveal } => env::exec(reveal),
        Command::Toolchain { action } => toolchain::exec(action),
        Command::SelfCmd { action } => self_::exec(action),
        Command::Build {
            target, profile, ..
        } => build::exec(target.as_deref(), profile.as_deref(), cli.verbose),
        Command::Add {
            dep,
            dev,
            target,
            flavor,
        } => add::exec(&dep, dev, target.as_deref(), flavor.as_deref()),
        Command::Remove {
            dep,
            dev,
            target,
            flavor,
        } => remove::exec(&dep, dev, target.as_deref(), flavor.as_deref()),
        Command::Fetch { verify } => fetch::exec(cli.verbose, verify),
        Command::Lock => lock::exec(cli.verbose),
        Command::Tree {
            depth,
            duplicates,
            inverted,
            why,
            conflicts,
            licenses,
        } => tree::exec(depth, duplicates, inverted, why, conflicts, licenses),
        Command::Outdated { major } => outdated::exec(major),
        Command::Update {
            major,
            dep,
            dry_run,
        } => update::exec(major, dep, dry_run),
        Command::Audit { fail_on } => audit::exec(fail_on),
        _ => {
            eprintln!("This command is not yet implemented");
            Ok(())
        }
    }
}
