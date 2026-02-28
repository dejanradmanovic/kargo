//! Repository authentication using credentials from `Kargo.toml`.
//!
//! Authentication is configured per-repository in `Kargo.toml` using
//! `${env:SECRET}` interpolation from `.kargo.env`:
//!
//! ```toml
//! [repositories]
//! my-private = { url = "https://nexus.co/maven", username = "${env:NEXUS_USER}", password = "${env:NEXUS_PASS}" }
//! ```
//!
//! By the time the manifest is loaded, `${env:...}` values are already
//! interpolated, so this module just reads the resolved credentials.

use reqwest::RequestBuilder;

use crate::repository::MavenRepository;

/// Apply authentication to a request if the repository has credentials.
pub fn apply_auth(request: RequestBuilder, repo: &MavenRepository) -> RequestBuilder {
    match (&repo.username, &repo.password) {
        (Some(user), Some(pass)) => request.basic_auth(user, Some(pass)),
        (Some(user), None) => request.basic_auth(user, None::<&str>),
        (None, Some(token)) => request.bearer_auth(token),
        (None, None) => request,
    }
}
