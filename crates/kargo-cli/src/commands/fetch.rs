//! Handler for `kargo fetch`.

use miette::Result;

pub async fn exec(verbose: bool, verify: bool) -> Result<()> {
    let project_root = std::env::current_dir().map_err(kargo_util::errors::KargoError::Io)?;

    if !project_root.join("Kargo.toml").is_file() {
        return Err(kargo_util::errors::KargoError::Manifest {
            message: "No Kargo.toml found in current directory".to_string(),
        }
        .into());
    }

    kargo_ops::ops_fetch::fetch(&project_root, verbose).await?;

    if verify {
        kargo_ops::ops_fetch::verify_checksums(&project_root)?;
    }

    Ok(())
}
