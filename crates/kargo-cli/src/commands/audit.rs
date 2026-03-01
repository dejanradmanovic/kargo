//! Handler for `kargo audit`.

use miette::Result;

use kargo_ops::ops_audit::{self, AuditOptions};

pub async fn exec(fail_on: Option<String>) -> Result<()> {
    let project_root = std::env::current_dir().map_err(kargo_util::errors::KargoError::Io)?;

    if !project_root.join("Kargo.toml").is_file() {
        return Err(kargo_util::errors::KargoError::Manifest {
            message: "No Kargo.toml found in current directory".to_string(),
        }
        .into());
    }

    let manifest_path = project_root.join("Kargo.toml");
    let ignore = load_audit_ignore(&manifest_path);

    let opts = AuditOptions { fail_on, ignore };

    ops_audit::audit(&project_root, &opts).await
}

/// Read `[audit] ignore = [...]` from Kargo.toml if present.
fn load_audit_ignore(manifest_path: &std::path::Path) -> Vec<String> {
    let content = match std::fs::read_to_string(manifest_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let doc: toml::Value = match content.parse() {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    doc.get("audit")
        .and_then(|a| a.get("ignore"))
        .and_then(|i| i.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}
