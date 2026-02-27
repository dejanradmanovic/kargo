use miette::Result;
use std::fs;

use kargo_util::errors::KargoError;
use kargo_util::fs::find_ancestor_with;

pub fn exec(variant: Option<&str>) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|e| KargoError::Io(e))?;
    let project_root = find_ancestor_with(&cwd, "Kargo.toml").ok_or_else(|| {
        KargoError::Manifest {
            message: "Could not find Kargo.toml in current or parent directories".to_string(),
        }
    })?;

    let build_dir = project_root.join("build");

    if let Some(variant_name) = variant {
        let variant_dir = build_dir.join("variants").join(variant_name);
        if variant_dir.exists() {
            fs::remove_dir_all(&variant_dir).map_err(|e| KargoError::Io(e))?;
            println!("Cleaned variant '{}'", variant_name);
        } else {
            println!("Variant '{}' build directory does not exist", variant_name);
        }
    } else if build_dir.exists() {
        fs::remove_dir_all(&build_dir).map_err(|e| KargoError::Io(e))?;
        println!("Cleaned build directory");
    } else {
        println!("Nothing to clean");
    }

    Ok(())
}
