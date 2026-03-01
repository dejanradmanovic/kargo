//! Self-management operations: info, clean, update.

use std::fs;

use miette::Result;

use kargo_core::config::GlobalConfig;
use kargo_toolchain::install;
use kargo_toolchain::sdk;
use kargo_util::fs::dir_size;

use crate::ops_self_update::{self, UpdateCheck};

pub fn cmd_info(pkg_version: &str) -> Result<()> {
    println!("Kargo {pkg_version}");
    println!();

    let config_path = GlobalConfig::default_path();
    println!(
        "  Config:         {} {}",
        config_path.display(),
        if config_path.is_file() {
            "(exists)"
        } else {
            "(not created)"
        }
    );

    let cache_dir = kargo_util::dirs_path().join("cache");
    let cache_size = dir_size(&cache_dir);
    println!(
        "  Cache:          {} ({})",
        cache_dir.display(),
        format_bytes(cache_size)
    );

    let toolchains = install::list_installed();
    let default = install::get_default();
    println!("  Toolchains:     {} installed", toolchains.len());
    if let Some(ref d) = default {
        println!("  Default Kotlin: {d}");
    }
    for v in &toolchains {
        let marker = if Some(v) == default.as_ref() {
            " *"
        } else {
            ""
        };
        println!("    - {v}{marker}");
    }

    let jdks = sdk::list_installed_jdks();
    if !jdks.is_empty() {
        println!("  Managed JDKs:   {} installed", jdks.len());
        for jdk in &jdks {
            println!("    - JDK {} at {}", jdk.version, jdk.home.display());
        }
    }

    let config = match GlobalConfig::load() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to load global config, using defaults: {e}");
            GlobalConfig::default()
        }
    };
    if let Some(jdk) = sdk::discover_jdk(config.toolchain.jdk.as_deref()) {
        println!(
            "  System JDK:     {} at {}",
            jdk.version,
            jdk.home.display()
        );
    }

    if let Some(android) = sdk::discover_android_sdk() {
        let platforms: Vec<String> = android
            .installed_platforms
            .iter()
            .map(|p| format!("android-{p}"))
            .collect();
        println!("  Android SDK:    {}", android.home.display());
        if !platforms.is_empty() {
            println!("    Platforms:    {}", platforms.join(", "));
        }
        if !android.installed_build_tools.is_empty() {
            println!(
                "    Build tools: {}",
                android.installed_build_tools.join(", ")
            );
        }
    }

    if let Some(xcode) = sdk::discover_xcode() {
        let ver = xcode.version.as_deref().unwrap_or("unknown");
        println!("  Xcode:          {} ({})", xcode.sdk_path.display(), ver);
    }

    Ok(())
}

pub fn cmd_clean() -> Result<()> {
    let mut total_freed: u64 = 0;

    let cache_dir = kargo_util::dirs_path().join("cache");
    if cache_dir.is_dir() {
        let size = dir_size(&cache_dir);
        fs::remove_dir_all(&cache_dir).map_err(kargo_util::errors::KargoError::Io)?;
        println!("  Removed dependency cache ({}).", format_bytes(size));
        total_freed += size;
    }

    let build_cache = kargo_util::dirs_path().join("build-cache");
    if build_cache.is_dir() {
        let size = dir_size(&build_cache);
        fs::remove_dir_all(&build_cache).map_err(kargo_util::errors::KargoError::Io)?;
        println!("  Removed build cache ({}).", format_bytes(size));
        total_freed += size;
    }

    if total_freed == 0 {
        println!("  Nothing to clean.");
    } else {
        println!("  Total freed: {}.", format_bytes(total_freed));
    }

    Ok(())
}

pub fn cmd_update(pkg_version: &str, check_only: bool) -> Result<()> {
    println!("  Kargo {pkg_version} (current)");
    println!("  Checking for updates...");

    match ops_self_update::check_for_update(pkg_version)? {
        UpdateCheck::UpToDate(v) => {
            println!("  Already up to date (v{v}).");
        }
        UpdateCheck::Available(info) => {
            println!("  Update available: {} -> {}", info.current, info.latest);

            if check_only {
                println!();
                println!("  Run `kargo self update` to install.");
                return Ok(());
            }

            ops_self_update::apply_update(&info)?;
            println!();
            println!("  Restart your shell or run `kargo --version` to verify.");
        }
    }

    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}
