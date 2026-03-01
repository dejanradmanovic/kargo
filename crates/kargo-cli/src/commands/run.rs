//! Run command implementation.

use miette::Result;

pub fn exec(target: Option<&str>, args: &[String], verbose: bool) -> Result<()> {
    let cwd = std::env::current_dir().map_err(kargo_util::errors::KargoError::Io)?;
    kargo_ops::ops_run::run(&cwd, target, args, verbose)
}
