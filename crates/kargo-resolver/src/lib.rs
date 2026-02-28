//! Dependency resolution engine: Maven-compatible nearest-wins algorithm,
//! transitive dependency resolution, scope propagation, and lockfile management.

pub mod cache;
pub mod conflict;
pub mod graph;
pub mod resolver;
pub mod version;
