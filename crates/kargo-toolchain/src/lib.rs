//! Kotlin toolchain management: compiler auto-download, version switching,
//! JDK discovery, SDK detection (Xcode, Android SDK).

pub mod discovery;
pub mod download;
pub mod install;
pub mod sdk;
pub mod version;
