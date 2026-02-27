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
