//! Handler for `kargo update`.

use miette::Result;

use kargo_ops::ops_update::{self, UpdateOptions};

pub fn exec(major: bool, dep: Option<String>, dry_run: bool) -> Result<()> {
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

    let rt =
        tokio::runtime::Runtime::new().map_err(|e| kargo_util::errors::KargoError::Generic {
            message: format!("Failed to start async runtime: {e}"),
        })?;

    rt.block_on(ops_update::update(&project_root, &opts))
}
