//! Shared utilities for the Kargo build tool.
//!
//! This crate provides cross-cutting concerns used by all other Kargo crates:
//! error types, filesystem helpers, cryptographic hashing, process spawning,
//! and terminal progress indicators.

pub mod errors;
pub mod fs;
pub mod hash;
pub mod process;
pub mod progress;

use std::path::{Path, PathBuf};

/// Returns the path to the Kargo data directory (`~/.kargo/`).
pub fn dirs_path() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    Path::new(&home).join(".kargo")
}
