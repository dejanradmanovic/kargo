//! Handler for `kargo update`.

use miette::Result;

use kargo_ops::ops_update::{self, UpdateOptions};

pub async fn exec(major: bool, dep: Option<String>, dry_run: bool) -> Result<()> {
    let project_root = std::env::current_dir().map_err(kargo_util::errors::KargoError::Io)?;

    if !project_root.join("Kargo.toml").is_file() {
        return Err(kargo_util::errors::KargoError::Manifest {
            message: "No Kargo.toml found in current directory".to_string(),
        }
        .into());
    }

    let opts = UpdateOptions {
        major,
        dep,
        dry_run,
    };

    ops_update::update(&project_root, &opts).await
}
