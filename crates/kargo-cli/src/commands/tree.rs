//! Handler for `kargo tree`.

use miette::Result;

use kargo_ops::ops_tree::{self, TreeOptions};

pub async fn exec(
    depth: Option<u32>,
    duplicates: bool,
    inverted: bool,
    why: Option<String>,
    conflicts: bool,
    licenses: bool,
) -> Result<()> {
    let project_root = std::env::current_dir().map_err(kargo_util::errors::KargoError::Io)?;

    if !project_root.join("Kargo.toml").is_file() {
        return Err(kargo_util::errors::KargoError::Manifest {
            message: "No Kargo.toml found in current directory".to_string(),
        }
        .into());
    }

    let opts = TreeOptions {
        depth: depth.map(|d| d as usize),
        why,
        duplicates,
        conflicts,
        licenses,
        inverted,
    };

    ops_tree::tree(&project_root, &opts).await
}
