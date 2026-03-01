//! Clean command implementation.

use miette::Result;

use kargo_ops::ops_clean::{self, CleanResult};
use kargo_util::errors::KargoError;
use kargo_util::fs::find_ancestor_with;

pub fn exec(variant: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir().map_err(KargoError::Io)?;
    let project_root =
        find_ancestor_with(&cwd, "Kargo.toml").ok_or_else(|| KargoError::Manifest {
            message: "Could not find Kargo.toml in current or parent directories".to_string(),
        })?;

    match ops_clean::clean(&project_root, variant)? {
        CleanResult::AllCleaned => println!("Cleaned build directory"),
        CleanResult::VariantCleaned(v) => println!("Cleaned variant '{v}'"),
        CleanResult::VariantNotFound(v) => println!("Variant '{v}' build directory does not exist"),
        CleanResult::NothingToClean => println!("Nothing to clean"),
    }

    Ok(())
}
