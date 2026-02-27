//! Core data types for the Kargo build tool.
//!
//! This crate defines the fundamental types that represent a Kargo project:
//! manifest parsing, packages, workspaces, targets, source sets, dependencies,
//! profiles, build flavors and variants, version catalogs, lockfiles,
//! configuration, and local properties.
//!
//! This crate is intentionally free of async code and network I/O.

/// Default Kotlin version used when scaffolding new projects.
pub const DEFAULT_KOTLIN_VERSION: &str = "2.3.0";

pub mod config;
pub mod dependency;
pub mod flavor;
pub mod lockfile;
pub mod manifest;
pub mod package;
pub mod profile;
pub mod properties;
pub mod source_set;
pub mod target;
pub mod template;
pub mod version_catalog;
pub mod workspace;
