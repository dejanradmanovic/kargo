//! Handler for `kargo outdated`.

use miette::Result;

use kargo_ops::ops_outdated::{self, OutdatedOptions};

pub async fn exec(major: bool) -> Result<()> {
    let project_root = std::env::current_dir().map_err(kargo_util::errors::KargoError::Io)?;

    if !project_root.join("Kargo.toml").is_file() {
        return Err(kargo_util::errors::KargoError::Manifest {
            message: "No Kargo.toml found in current directory".to_string(),
        }
        .into());
    }

    let opts = OutdatedOptions { major };

    ops_outdated::outdated(&project_root, &opts).await
}
