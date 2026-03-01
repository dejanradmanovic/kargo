//! Build command implementation.

use miette::Result;

use kargo_ops::ops_build::{self, BuildOptions};

pub fn exec(
    target: Option<&str>,
    profile: Option<&str>,
    release: bool,
    timings: bool,
    offline: bool,
    verbose: bool,
) -> Result<()> {
    let cwd = std::env::current_dir().map_err(kargo_util::errors::KargoError::Io)?;

    let opts = BuildOptions {
        target: target.map(String::from),
        profile: profile.map(String::from),
        release,
        verbose,
        timings,
        offline,
        ..Default::default()
    };

    let result = ops_build::build(&cwd, &opts)?;

    if !result.success {
        std::process::exit(1);
    }

    Ok(())
}
