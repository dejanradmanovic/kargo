use crate::manifest::CatalogConfig;

/// Resolved version catalog entry with the actual version string.
#[derive(Debug, Clone)]
pub struct ResolvedCatalogEntry {
    pub group: String,
    pub artifact: String,
    pub version: String,
}

/// Resolve all catalog library entries by substituting version refs.
pub fn resolve_catalog(catalog: &CatalogConfig) -> Vec<(String, ResolvedCatalogEntry)> {
    let mut entries = Vec::new();
    for (name, lib) in &catalog.libraries {
        let version = if let Some(ref vref) = lib.version_ref {
            catalog.versions.get(vref).cloned().unwrap_or_default()
        } else {
            lib.version.clone().unwrap_or_default()
        };
        entries.push((
            name.clone(),
            ResolvedCatalogEntry {
                group: lib.group.clone(),
                artifact: lib.artifact.clone(),
                version,
            },
        ));
    }
    entries
}
