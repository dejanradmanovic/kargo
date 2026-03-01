//! Test command implementation.

use miette::Result;

pub async fn exec(target: Option<&str>, filter: Option<&str>, verbose: bool) -> Result<()> {
    let cwd = std::env::current_dir().map_err(kargo_util::errors::KargoError::Io)?;
    kargo_ops::ops_test::test(&cwd, target, filter, verbose).await
}
