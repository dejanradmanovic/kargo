//! Publishing artifacts to Maven repositories with GPG signing.
//!
//! This module will be implemented in Phase 8 (Packaging & Distribution).

use crate::repository::MavenRepository;

/// Publish a JAR + POM + sources to a Maven repository.
///
/// # Not yet implemented
///
/// This is a Phase 8 feature. Calling this will return an error.
pub async fn publish_artifact(
    _repo: &MavenRepository,
    _group: &str,
    _artifact: &str,
    _version: &str,
) -> miette::Result<()> {
    Err(kargo_util::errors::KargoError::Generic {
        message: "Publishing is not yet implemented (planned for Phase 8)".to_string(),
    }
    .into())
}
