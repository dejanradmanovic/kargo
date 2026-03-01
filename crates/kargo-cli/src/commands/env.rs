use miette::Result;

use kargo_core::properties::load_env_file;
use kargo_util::errors::KargoError;
use kargo_util::fs::find_ancestor_with;

pub fn exec(reveal: bool) -> Result<()> {
    let cwd = std::env::current_dir().map_err(KargoError::Io)?;
    let project_root =
        find_ancestor_with(&cwd, "Kargo.toml").ok_or_else(|| KargoError::Manifest {
            message: "Could not find Kargo.toml in this directory or any parent".to_string(),
        })?;

    let env_vars = load_env_file(&project_root.join(".kargo.env"))?;

    if env_vars.is_empty() {
        println!("No environment variables configured.");
        println!(
            "  .kargo.env: {}",
            project_root.join(".kargo.env").display()
        );
        return Ok(());
    }

    println!(".kargo.env ({} entries):", env_vars.len());
    for (key, value) in &env_vars {
        let display_value = if reveal { value.as_str() } else { "********" };
        println!("  {} = {}", key, display_value);
    }

    Ok(())
}
