//! Operation: remove build artifacts and compiler metadata.

use std::path::Path;

use kargo_util::errors::KargoError;

/// Remove build artifacts and associated compiler metadata from the project.
///
/// If `variant` is provided (e.g. "jvm/dev" or "jvm/release"), only that
/// variant's build directory `build/<variant>` is removed.
/// Otherwise the entire `build/` directory plus `.kargo/fingerprints/` are
/// removed so the next build runs completely fresh.
pub fn clean(project_dir: &Path, variant: Option<&str>) -> miette::Result<CleanResult> {
    let build_dir = project_dir.join("build");

    if let Some(variant_name) = variant {
        let variant_dir = build_dir.join(variant_name);
        if variant_dir.exists() {
            std::fs::remove_dir_all(&variant_dir).map_err(KargoError::Io)?;
            Ok(CleanResult::VariantCleaned(variant_name.to_string()))
        } else {
            Ok(CleanResult::VariantNotFound(variant_name.to_string()))
        }
    } else if build_dir.exists() {
        std::fs::remove_dir_all(&build_dir).map_err(KargoError::Io)?;
        remove_if_exists(&project_dir.join(".kargo").join("fingerprints"));
        Ok(CleanResult::AllCleaned)
    } else {
        Ok(CleanResult::NothingToClean)
    }
}

fn remove_if_exists(path: &Path) {
    if path.exists() {
        if let Err(e) = std::fs::remove_dir_all(path) {
            tracing::warn!("Failed to remove directory {}: {e}", path.display());
        }
    }
}

/// Result of a clean operation.
pub enum CleanResult {
    AllCleaned,
    VariantCleaned(String),
    VariantNotFound(String),
    NothingToClean,
}
