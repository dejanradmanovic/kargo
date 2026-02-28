//! Handler for `kargo fetch`.

use miette::Result;

pub fn exec(verbose: bool) -> Result<()> {
    let project_root = std::env::current_dir().map_err(kargo_util::errors::KargoError::Io)?;

    if !project_root.join("Kargo.toml").is_file() {
        return Err(kargo_util::errors::KargoError::Manifest {
            message: "No Kargo.toml found in current directory".to_string(),
        }
        .into());
    }

    let rt =
        tokio::runtime::Runtime::new().map_err(|e| kargo_util::errors::KargoError::Generic {
            message: format!("Failed to start async runtime: {e}"),
        })?;

    rt.block_on(kargo_ops::ops_fetch::fetch(&project_root, verbose))
}
