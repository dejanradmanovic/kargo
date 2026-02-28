//! Toolchain and SDK setup shared by `new`, `init`, and `build`.
//!
//! Two entry points:
//! - [`post_scaffold`] — interactive, best-effort (used by `new`/`init`)
//! - [`preflight`] — strict, returns errors if something is missing (used by `build`)

use std::path::Path;

use kargo_core::config::GlobalConfig;
use kargo_core::manifest::Manifest;
use kargo_toolchain::discovery::ToolchainPaths;
use kargo_toolchain::install;
use kargo_toolchain::sdk;
use kargo_toolchain::version::KotlinVersion;
use kargo_util::errors::KargoError;

// ---------------------------------------------------------------------------
// Pre-build preflight (strict)
// ---------------------------------------------------------------------------

/// Result of a successful preflight check.
pub struct PreflightResult {
    pub toolchain: ToolchainPaths,
    pub jdk: sdk::JdkInfo,
    pub java_target: String,
    pub android_sdk: Option<sdk::AndroidSdkInfo>,
    pub xcode: Option<sdk::XcodeInfo>,
}

/// Verify that all required toolchains and SDKs are available before building.
///
/// Unlike `post_scaffold`, this function returns hard errors when something
/// critical is missing. It will auto-download the Kotlin compiler if
/// `auto_download` is enabled in the global config, but will not interactively
/// prompt the user for JDK/SDK installation — those must already be present.
pub fn preflight(project_dir: &Path) -> miette::Result<PreflightResult> {
    let manifest = load_manifest(project_dir)?;
    let config = GlobalConfig::load().unwrap_or_default();
    let mirror = config.toolchain.kotlin_mirror.as_deref();

    // 1. Kotlin compiler
    let version: KotlinVersion =
        manifest
            .package
            .kotlin
            .parse()
            .map_err(|e| KargoError::Toolchain {
                message: format!("Invalid kotlin version '{}': {e}", manifest.package.kotlin),
            })?;

    let toolchain = kargo_toolchain::discovery::resolve_toolchain(
        &version,
        config.toolchain.auto_download,
        mirror,
    )?;

    // 2. JDK (always required for Kotlin compilation)
    let java_target = manifest
        .targets
        .values()
        .find_map(|tc| tc.java_target.as_deref())
        .unwrap_or("21");
    let required_major: u32 = java_target.parse().unwrap_or(21);

    let jdk = sdk::discover_jdk_for_target(config.toolchain.jdk.as_deref(), required_major)
        .ok_or_else(|| {
            // Check if there's *any* JDK to give a better error message
            let hint = match sdk::discover_jdk(config.toolchain.jdk.as_deref()) {
                Some(found) => format!(
                    "\n  Found JDK {} at {}, but java-target requires >= {java_target}.",
                    found.version,
                    found.home.display()
                ),
                None => String::new(),
            };
            KargoError::Toolchain {
                message: format!(
                    "No JDK >= {java_target} found. Kotlin requires a compatible JDK.{hint}\n  \
                 Set JAVA_HOME, configure [toolchain].jdk in ~/.kargo/config.toml,\n  \
                 or install one with: kargo toolchain install --jdk {java_target}"
                ),
            }
        })?;

    // 3. Target-specific checks
    let has_android = manifest.targets.keys().any(|k| k == "android");
    let has_apple = manifest
        .targets
        .keys()
        .any(|k| k.starts_with("ios") || k.starts_with("macos"));

    let android_sdk = if has_android {
        let compile_sdk = manifest
            .targets
            .get("android")
            .and_then(|tc| tc.compile_sdk)
            .unwrap_or(35);

        let info = sdk::discover_android_sdk().ok_or_else(|| KargoError::Toolchain {
            message: format!(
                "Android SDK not found (required for android target, compile-sdk {compile_sdk}).\n  \
                 Set ANDROID_HOME or install with: kargo toolchain install --android"
            ),
        })?;

        // Validate that the required platform is installed; auto-install if possible
        if !sdk::has_platform(&info, compile_sdk) {
            sdk::ensure_android_components(&info, compile_sdk)?;
        }

        Some(info)
    } else {
        None
    };

    let xcode = if has_apple {
        let info = sdk::discover_xcode().ok_or_else(|| KargoError::Toolchain {
            message: "Xcode not found (required for iOS/macOS targets).\n  \
                      Install from the App Store or run: xcode-select --install"
                .to_string(),
        })?;
        Some(info)
    } else {
        None
    };

    Ok(PreflightResult {
        toolchain,
        jdk,
        java_target: java_target.to_string(),
        android_sdk,
        xcode,
    })
}

/// Print a summary of the preflight result.
pub fn print_preflight_summary(result: &PreflightResult) {
    println!(
        "  Kotlin {} at {}",
        result.toolchain.version,
        result.toolchain.home.display()
    );
    println!(
        "  JDK {} at {} (java-target: {})",
        result.jdk.version,
        result.jdk.home.display(),
        result.java_target,
    );
    if let Some(ref android) = result.android_sdk {
        let platforms: Vec<String> = android
            .installed_platforms
            .iter()
            .map(|p| p.to_string())
            .collect();
        println!(
            "  Android SDK at {} (platforms: {})",
            android.home.display(),
            if platforms.is_empty() {
                "none".to_string()
            } else {
                platforms.join(", ")
            }
        );
    }
    if let Some(ref xcode) = result.xcode {
        let ver = xcode.version.as_deref().unwrap_or("unknown");
        println!("  Xcode {ver} at {}", xcode.sdk_path.display());
    }
}

// ---------------------------------------------------------------------------
// Post-scaffold (interactive, best-effort)
// ---------------------------------------------------------------------------

/// Run toolchain and SDK setup after project scaffolding.
///
/// Called after `kargo new` and `kargo init`. All errors are non-fatal
/// (printed as warnings) — the project is always created regardless.
pub fn post_scaffold(project_dir: &Path) {
    println!();
    println!("  Setting up toolchain...");

    let manifest_path = project_dir.join("Kargo.toml");
    if !manifest_path.is_file() {
        return;
    }

    let config = GlobalConfig::load().unwrap_or_default();
    let mirror = config.toolchain.kotlin_mirror.as_deref();

    setup_kotlin(&manifest_path, &config, mirror);

    let manifest_content = match std::fs::read_to_string(&manifest_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let manifest = match Manifest::parse_toml(&manifest_content) {
        Ok(m) => m,
        Err(_) => return,
    };

    setup_jdk(&config, &manifest);
    setup_target_sdks(&manifest);

    resolve_lockfile(project_dir);

    println!();
    println!("  Ready to build!");
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn load_manifest(project_dir: &Path) -> miette::Result<Manifest> {
    let manifest_path = kargo_util::fs::find_ancestor_with(project_dir, "Kargo.toml")
        .map(|d| d.join("Kargo.toml"))
        .ok_or_else(|| KargoError::Toolchain {
            message: "No Kargo.toml found in this directory or any parent".to_string(),
        })?;

    Manifest::from_path(&manifest_path)
}

fn setup_kotlin(manifest_path: &Path, config: &GlobalConfig, mirror: Option<&str>) {
    let version = match KotlinVersion::from_manifest(manifest_path) {
        Ok(v) => v,
        Err(e) => {
            println!("  Warning: could not read Kotlin version: {e}");
            return;
        }
    };

    if install::is_installed(&version) {
        println!("  Kotlin {} already installed.", version);
    } else if config.toolchain.auto_download {
        match install::install_kotlin(&version, mirror) {
            Ok(_) => {}
            Err(e) => {
                println!("  Warning: failed to install Kotlin {version}: {e}");
            }
        }
    } else {
        println!(
            "  Kotlin {} is not installed. Run: kargo toolchain install {}",
            version, version
        );
    }

    if install::get_default().is_none() {
        let _ = install::set_default(&version);
    }
}

fn setup_jdk(config: &GlobalConfig, manifest: &Manifest) {
    let java_target = manifest
        .targets
        .values()
        .find_map(|tc| tc.java_target.as_deref())
        .unwrap_or("21");
    let required_major: u32 = java_target.parse().unwrap_or(21);

    match sdk::discover_jdk_for_target(config.toolchain.jdk.as_deref(), required_major) {
        Some(jdk) => {
            println!(
                "  JDK {} found at {} (satisfies >= {java_target})",
                jdk.version,
                jdk.home.display()
            );
        }
        None => {
            // Check if there's a JDK but wrong version
            if let Some(found) = sdk::discover_jdk(config.toolchain.jdk.as_deref()) {
                println!(
                    "  JDK {} found but java-target requires >= {java_target}.",
                    found.version
                );
            }
            match sdk::prompt_and_install_jdk(java_target) {
                Ok(jdk) => {
                    println!("  JDK {} ready.", jdk.version);
                }
                Err(e) => {
                    println!("  Warning: JDK not available: {e}");
                    println!(
                        "  Set JAVA_HOME or install with: kargo toolchain install --jdk {java_target}"
                    );
                }
            }
        }
    }
}

fn setup_target_sdks(manifest: &Manifest) {
    let has_android = manifest.targets.keys().any(|k| k == "android");
    let has_ios = manifest
        .targets
        .keys()
        .any(|k| k.starts_with("ios") || k.starts_with("macos"));

    if has_android {
        let compile_sdk = manifest
            .targets
            .get("android")
            .and_then(|tc| tc.compile_sdk)
            .unwrap_or(35);

        match sdk::discover_android_sdk() {
            Some(android) => {
                println!("  Android SDK found at {}", android.home.display());
                if sdk::has_platform(&android, compile_sdk) {
                    println!("  android-{compile_sdk} platform installed.");
                } else {
                    println!("  android-{compile_sdk} platform missing, installing...");
                    if let Err(e) = sdk::ensure_android_components(&android, compile_sdk) {
                        println!("  Warning: could not install android-{compile_sdk}: {e}");
                    }
                }
            }
            None => match sdk::prompt_and_install_android_sdk(compile_sdk) {
                Ok(android) => {
                    println!("  Android SDK ready at {}", android.home.display());
                }
                Err(_) => {
                    println!("  Warning: Android SDK not configured.");
                    println!(
                        "  Set ANDROID_HOME or install with: kargo toolchain install --android"
                    );
                }
            },
        }
    }

    if has_ios {
        match sdk::discover_xcode() {
            Some(xcode) => {
                let ver = xcode.version.as_deref().unwrap_or("unknown");
                println!("  Xcode {} found at {}", ver, xcode.sdk_path.display());
            }
            None => {
                println!("  Warning: Xcode not found (required for iOS/macOS targets).");
                sdk::print_xcode_instructions();
            }
        }
    }
}

/// Resolve dependencies and generate `Kargo.lock` after scaffolding.
///
/// Best-effort: errors are printed as warnings but never block project creation.
fn resolve_lockfile(project_dir: &Path) {
    let manifest_path = project_dir.join("Kargo.toml");
    let manifest = match Manifest::from_path(&manifest_path) {
        Ok(m) => m,
        Err(_) => return,
    };

    if manifest.dependencies.is_empty() && manifest.dev_dependencies.is_empty() {
        return;
    }

    println!("  Resolving dependencies...");

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            println!("  Warning: could not start async runtime: {e}");
            return;
        }
    };

    if let Err(e) = rt.block_on(crate::ops_fetch::fetch(project_dir, false)) {
        println!("  Warning: failed to resolve dependencies: {e}");
    }
}

/// Ensure the lockfile is present and up-to-date before building.
///
/// Called during `preflight`. If the lockfile is missing or stale, triggers
/// a fresh resolution. Also verifies cached JAR checksums against the lockfile
/// to detect corruption or tampering.
pub fn ensure_lockfile(project_dir: &Path) -> miette::Result<()> {
    let manifest_path = project_dir.join("Kargo.toml");
    let manifest = Manifest::from_path(&manifest_path)?;

    if manifest.dependencies.is_empty() && manifest.dev_dependencies.is_empty() {
        return Ok(());
    }

    let lockfile_path = project_dir.join("Kargo.lock");
    let needs_resolve = if lockfile_path.is_file() {
        match kargo_core::lockfile::Lockfile::from_path(&lockfile_path) {
            Ok(lf) => {
                let declared = crate::ops_fetch::collect_declared_deps(&manifest);
                !lf.is_up_to_date(&declared)
            }
            Err(_) => true,
        }
    } else {
        true
    };

    if needs_resolve {
        let rt = tokio::runtime::Runtime::new().map_err(|e| KargoError::Generic {
            message: format!("Failed to start async runtime: {e}"),
        })?;
        rt.block_on(crate::ops_fetch::fetch(project_dir, false))?;
    }

    // Verify cached JAR checksums against the lockfile
    if let Ok(lf) = kargo_core::lockfile::Lockfile::from_path(&lockfile_path) {
        verify_cached_checksums(project_dir, &lf)?;
    }

    Ok(())
}

/// Verify that cached JARs match the checksums recorded in `Kargo.lock`.
///
/// Skips entries without a recorded checksum or without a cached JAR.
fn verify_cached_checksums(
    project_dir: &Path,
    lockfile: &kargo_core::lockfile::Lockfile,
) -> miette::Result<()> {
    let cache = kargo_maven::cache::LocalCache::new(project_dir);

    for pkg in &lockfile.package {
        let expected = match &pkg.checksum {
            Some(c) if !c.is_empty() => c,
            _ => continue,
        };

        let jar_path = match cache.get_jar(&pkg.group, &pkg.name, &pkg.version, None) {
            Some(p) => p,
            None => continue,
        };

        let data = std::fs::read(&jar_path).map_err(|e| KargoError::Generic {
            message: format!(
                "Failed to read cached JAR {}:{}:{}: {e}",
                pkg.group, pkg.name, pkg.version
            ),
        })?;

        let actual = kargo_util::hash::sha256_bytes(&data);
        if actual != *expected {
            return Err(KargoError::Generic {
                message: format!(
                    "Checksum mismatch for {}:{}:{}\n  \
                     expected: {expected}\n  \
                     actual:   {actual}\n\
                     The cached JAR may be corrupted. \
                     Run `kargo fetch` to re-download.",
                    pkg.group, pkg.name, pkg.version
                ),
            }
            .into());
        }
    }

    Ok(())
}
