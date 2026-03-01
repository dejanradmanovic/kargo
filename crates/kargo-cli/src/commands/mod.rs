//! Command dispatch and handler modules.

mod add;
mod audit;
mod build;
mod cache;
mod check;
mod clean;
mod env;
mod fetch;
mod init;
mod lock;
mod new;
mod outdated;
mod remove;
mod run;
mod self_;
mod test_;
mod toolchain;
mod tree;
mod update;
mod watch;

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
            target,
            profile,
            release,
            timings,
            offline,
            ..
        } => build::exec(
            target.as_deref(),
            profile.as_deref(),
            release,
            timings,
            offline,
            cli.verbose,
        ),
        Command::Run { target, args, .. } => run::exec(target.as_deref(), &args, cli.verbose),
        Command::Test { target, filter, .. } => {
            test_::exec(target.as_deref(), filter.as_deref(), cli.verbose)
        }
        Command::Check { .. } => check::exec(cli.verbose),
        Command::Cache { action } => cache::exec(action),
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
        Command::Watch { build_only } => watch::exec(build_only, cli.verbose),
        _ => {
            eprintln!("This command is not yet implemented");
            Ok(())
        }
    }
}
