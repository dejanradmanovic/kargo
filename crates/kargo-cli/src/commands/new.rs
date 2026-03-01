use miette::Result;
use std::path::Path;

use kargo_core::template::{TemplateContext, TemplateRegistry};
use kargo_core::DEFAULT_KOTLIN_VERSION;
use kargo_util::errors::KargoError;

pub async fn exec(name: &str, template: &str) -> Result<()> {
    let project_dir = Path::new(name);
    if project_dir.exists() {
        return Err(KargoError::Generic {
            message: format!("Directory '{}' already exists", name),
        }
        .into());
    }

    let registry = TemplateRegistry::new()?;
    let tmpl = registry.get(template).ok_or_else(|| KargoError::Generic {
        message: format!(
            "Unknown template '{}'. Available: {}",
            template,
            registry.names().join(", ")
        ),
    })?;

    std::fs::create_dir_all(project_dir).map_err(KargoError::Io)?;

    let ctx = TemplateContext::new(name, DEFAULT_KOTLIN_VERSION);
    tmpl.render(project_dir, &ctx)?;

    println!(
        "Created new Kargo project '{}' with template '{}'",
        name, template
    );

    kargo_ops::ops_setup::post_scaffold(project_dir).await;

    Ok(())
}
