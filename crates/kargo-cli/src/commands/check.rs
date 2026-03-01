//! Check command implementation.

use miette::Result;

pub fn exec(verbose: bool) -> Result<()> {
    let cwd = std::env::current_dir().map_err(kargo_util::errors::KargoError::Io)?;
    kargo_ops::ops_check::check(&cwd, verbose)
}
