//! Maven repository protocol: POM parsing, artifact download, checksum
//! verification, local cache, and authentication.

pub mod auth;
pub mod cache;
pub mod checksum;
pub mod download;
pub mod metadata;
pub mod pom;
pub mod publish;
pub mod repository;
