//! Handler for `kargo lock`.

use miette::Result;

pub async fn exec(verbose: bool) -> Result<()> {
    let project_root = std::env::current_dir().map_err(kargo_util::errors::KargoError::Io)?;

    if !project_root.join("Kargo.toml").is_file() {
        return Err(kargo_util::errors::KargoError::Manifest {
            message: "No Kargo.toml found in current directory".to_string(),
        }
        .into());
    }

    kargo_ops::ops_lock::lock(&project_root, verbose).await
}
