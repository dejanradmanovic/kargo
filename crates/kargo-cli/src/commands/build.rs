//! Build command implementation.

use miette::Result;

pub fn exec(target: Option<&str>, profile: Option<&str>, verbose: bool) -> Result<()> {
    let cwd = std::env::current_dir().map_err(kargo_util::errors::KargoError::Io)?;

    let result = kargo_ops::ops_setup::preflight(&cwd)?;

    if verbose {
        kargo_ops::ops_setup::print_preflight_summary(&result);
        println!();
    }

    kargo_ops::ops_setup::ensure_lockfile(&cwd)?;

    eprintln!(
        "kargo build is not yet implemented (Kotlin {} ready, JDK {} available)",
        result.toolchain.version, result.jdk.version
    );

    if let Some(t) = target {
        eprintln!("  target: {t}");
    }
    if let Some(p) = profile {
        eprintln!("  profile: {p}");
    }

    Ok(())
}
