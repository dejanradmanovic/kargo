//! Handler for `kargo add`.

use miette::Result;

use kargo_ops::ops_add::{self, AddOptions};

pub fn exec(dep: &str, dev: bool, target: Option<&str>, flavor: Option<&str>) -> Result<()> {
    let project_root = std::env::current_dir().map_err(kargo_util::errors::KargoError::Io)?;
    let manifest_path = project_root.join("Kargo.toml");

    if !manifest_path.is_file() {
        return Err(kargo_util::errors::KargoError::Manifest {
            message: "No Kargo.toml found in current directory".to_string(),
        }
        .into());
    }

    ops_add::add_dependency(
        &manifest_path,
        &AddOptions {
            spec: dep.to_string(),
            dev,
            target: target.map(|s| s.to_string()),
            flavor: flavor.map(|s| s.to_string()),
        },
    )?;

    if dev {
        eprintln!("Added {dep} to [dev-dependencies]");
    } else if let Some(t) = target {
        eprintln!("Added {dep} to [target.{t}.dependencies]");
    } else if let Some(f) = flavor {
        eprintln!("Added {dep} to [flavor.{f}.dependencies]");
    } else {
        eprintln!("Added {dep} to [dependencies]");
    }

    let rt =
        tokio::runtime::Runtime::new().map_err(|e| kargo_util::errors::KargoError::Generic {
            message: format!("Failed to start async runtime: {e}"),
        })?;
    rt.block_on(kargo_ops::ops_fetch::fetch(&project_root, false))?;

    Ok(())
}
