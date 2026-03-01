//! Handler for `kargo remove`.

use miette::Result;

use kargo_ops::ops_remove::{self, RemoveOptions};

pub async fn exec(dep: &str, dev: bool, target: Option<&str>, flavor: Option<&str>) -> Result<()> {
    let project_root = std::env::current_dir().map_err(kargo_util::errors::KargoError::Io)?;
    let manifest_path = project_root.join("Kargo.toml");

    if !manifest_path.is_file() {
        return Err(kargo_util::errors::KargoError::Manifest {
            message: "No Kargo.toml found in current directory".to_string(),
        }
        .into());
    }

    ops_remove::remove_dependency(
        &manifest_path,
        &RemoveOptions {
            name: dep.to_string(),
            dev,
            target: target.map(|s| s.to_string()),
            flavor: flavor.map(|s| s.to_string()),
        },
    )?;

    eprintln!("Removed {dep}");

    kargo_ops::ops_fetch::fetch(&project_root, false).await?;

    Ok(())
}
