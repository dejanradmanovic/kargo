use miette::Result;

use kargo_core::template::{TemplateContext, TemplateRegistry};
use kargo_core::DEFAULT_KOTLIN_VERSION;
use kargo_util::errors::KargoError;

pub fn exec(template: &str) -> Result<()> {
    let cwd = std::env::current_dir().map_err(KargoError::Io)?;
    let manifest_path = cwd.join("Kargo.toml");

    if manifest_path.exists() {
        return Err(KargoError::Generic {
            message: "Kargo.toml already exists in this directory".to_string(),
        }
        .into());
    }

    let name = cwd
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("my-project");

    let registry = TemplateRegistry::new()?;
    let tmpl = registry.get(template).ok_or_else(|| KargoError::Generic {
        message: format!(
            "Unknown template '{}'. Available: {}",
            template,
            registry.names().join(", ")
        ),
    })?;

    let ctx = TemplateContext::new(name, DEFAULT_KOTLIN_VERSION);
    tmpl.render_core_only(&cwd, &ctx)?;

    println!("Initialized Kargo project in {}", cwd.display());

    kargo_ops::ops_setup::post_scaffold(&cwd);

    Ok(())
}
