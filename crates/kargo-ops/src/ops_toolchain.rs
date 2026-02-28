//! Toolchain management operations: install, list, remove, use, path.

use std::path::PathBuf;

use miette::Result;

use kargo_core::config::GlobalConfig;
use kargo_core::manifest::Manifest;
use kargo_toolchain::install;
use kargo_toolchain::sdk;
use kargo_toolchain::version::KotlinVersion;

fn try_load_project_manifest() -> Option<Manifest> {
    let cwd = std::env::current_dir().ok()?;
    let manifest_dir = kargo_util::fs::find_ancestor_with(&cwd, "Kargo.toml")?;
    Manifest::from_path(&manifest_dir.join("Kargo.toml")).ok()
}

pub fn cmd_install(
    version_str: Option<&str>,
    jdk_version: Option<&str>,
    android_version: Option<&str>,
) -> Result<()> {
    let manifest = try_load_project_manifest();
    let wants_jdk = jdk_version.is_some();
    let wants_android = android_version.is_some();

    let kotlin_version: Option<String> = match version_str {
        Some(v) => Some(v.to_string()),
        None => manifest.as_ref().map(|m| m.package.kotlin.clone()),
    };

    if kotlin_version.is_none() && !wants_jdk && !wants_android {
        return Err(kargo_util::errors::KargoError::Generic {
            message: "Provide a Kotlin version to install, or use flags for SDKs.\n  \
                      Examples:\n    \
                        kargo toolchain install 2.3.0\n    \
                        kargo toolchain install --jdk\n    \
                        kargo toolchain install --jdk 17\n    \
                        kargo toolchain install --android\n    \
                        kargo toolchain install --android 34\n    \
                        kargo toolchain install 2.3.0 --jdk --android\n\n  \
                      Or run inside a project directory to use versions from Kargo.toml."
                .to_string(),
        }
        .into());
    }

    if let Some(ver_str) = &kotlin_version {
        let version: KotlinVersion =
            ver_str
                .parse()
                .map_err(|e| kargo_util::errors::KargoError::Toolchain {
                    message: format!("Invalid version '{ver_str}': {e}"),
                })?;

        let from = if version_str.is_some() {
            "argument"
        } else {
            "Kargo.toml"
        };

        let config = GlobalConfig::load()?;
        let mirror = config.toolchain.kotlin_mirror.as_deref();

        if install::is_installed(&version) {
            println!("  Kotlin {version} already installed (from {from}).");
        } else {
            println!("  Installing Kotlin {version} (from {from})...");
            install::install_kotlin(&version, mirror)?;
        }

        if install::get_default().is_none() {
            install::set_default(&version)?;
            println!("  Set as default Kotlin version.");
        }
    }

    if wants_jdk {
        let explicit_jdk = jdk_version.unwrap_or("21");
        let manifest_java_target = manifest
            .as_ref()
            .and_then(|m| m.targets.values().find_map(|tc| tc.java_target.clone()));

        let (java_ver, from) = if explicit_jdk != "21" {
            (explicit_jdk.to_string(), "argument")
        } else if let Some(ref target) = manifest_java_target {
            (target.clone(), "Kargo.toml")
        } else {
            ("21".to_string(), "default")
        };

        let required: u32 = java_ver.parse().unwrap_or(21);

        match sdk::discover_jdk_for_target(None, required) {
            Some(existing) => {
                println!(
                    "  JDK {} already available at {} (satisfies >= {required}, from {from})",
                    existing.version,
                    existing.home.display()
                );
            }
            None => {
                println!("  JDK >= {required} required (from {from}).");
                sdk::prompt_and_install_jdk(&java_ver)?;
            }
        }
    }

    if wants_android {
        let explicit_android = android_version.unwrap_or("35");

        let manifest_compile_sdk = manifest
            .as_ref()
            .and_then(|m| m.targets.get("android"))
            .and_then(|tc| tc.compile_sdk);

        let (compile_sdk, from) = if explicit_android != "35" {
            (explicit_android.parse::<u32>().unwrap_or(35), "argument")
        } else if let Some(sdk_level) = manifest_compile_sdk {
            (sdk_level, "Kargo.toml")
        } else {
            (35, "default")
        };

        match sdk::discover_android_sdk() {
            Some(info) => {
                println!("  Android SDK found at {}", info.home.display());
                if sdk::has_platform(&info, compile_sdk) {
                    println!("  android-{compile_sdk} already installed (from {from}).");
                } else {
                    println!("  android-{compile_sdk} missing (from {from}), installing...");
                    sdk::ensure_android_components(&info, compile_sdk)?;
                }
            }
            None => {
                println!(
                    "  Android SDK not found. Installing with compile-sdk {compile_sdk} (from {from})..."
                );
                sdk::prompt_and_install_android_sdk(compile_sdk)?;
            }
        }
    }

    Ok(())
}

pub fn cmd_list() -> Result<()> {
    let versions = install::list_installed();
    let default = install::get_default();

    if versions.is_empty() {
        println!("No Kotlin toolchains installed.");
        println!("  Install one with: kargo toolchain install <version>");
        return Ok(());
    }

    println!("Installed Kotlin toolchains:");
    for v in &versions {
        let marker = if Some(v) == default.as_ref() {
            " (default)"
        } else {
            ""
        };
        let path = install::toolchain_dir(v);
        println!("  {v}{marker}  {}", path.display());
    }

    let jdks = sdk::list_installed_jdks();
    if !jdks.is_empty() {
        println!();
        println!("Managed JDKs:");
        for jdk in &jdks {
            println!("  JDK {} at {}", jdk.version, jdk.home.display());
        }
    }

    if let Some(android) = sdk::discover_android_sdk() {
        println!();
        println!("Android SDK at {}:", android.home.display());
        if !android.installed_platforms.is_empty() {
            let platforms: Vec<String> = android
                .installed_platforms
                .iter()
                .map(|p| format!("android-{p}"))
                .collect();
            println!("  Platforms: {}", platforms.join(", "));
        }
        if !android.installed_build_tools.is_empty() {
            println!(
                "  Build tools: {}",
                android.installed_build_tools.join(", ")
            );
        }
    }

    Ok(())
}

pub fn cmd_remove(
    version_str: Option<&str>,
    jdk_version: Option<&str>,
    android: bool,
) -> Result<()> {
    if version_str.is_none() && jdk_version.is_none() && !android {
        return Err(kargo_util::errors::KargoError::Generic {
            message: "Specify what to remove.\n  \
                      Examples:\n    \
                        kargo toolchain remove 2.3.0        (remove Kotlin)\n    \
                        kargo toolchain remove --jdk 21     (remove managed JDK)\n    \
                        kargo toolchain remove --android    (remove managed Android SDK)"
                .to_string(),
        }
        .into());
    }

    if let Some(ver) = version_str {
        let version: KotlinVersion =
            ver.parse()
                .map_err(|e| kargo_util::errors::KargoError::Toolchain {
                    message: format!("Invalid version '{ver}': {e}"),
                })?;

        let is_default = install::get_default().as_ref() == Some(&version);
        install::uninstall_kotlin(&version)?;
        println!("  Removed Kotlin {version}.");

        if is_default {
            println!("  Note: this was the default version. Set a new default with: kargo toolchain use <version>");
        }
    }

    if let Some(jdk_ver) = jdk_version {
        sdk::remove_jdk(jdk_ver)?;
    }

    if android {
        let dir = sdk::managed_android_sdk_dir();
        if dir.is_dir() {
            std::fs::remove_dir_all(&dir).map_err(kargo_util::errors::KargoError::Io)?;
            println!("  Removed managed Android SDK at {}", dir.display());
        } else {
            println!(
                "  No managed Android SDK found at {}. \
                 Only Kargo-managed SDKs can be removed.",
                dir.display()
            );
        }
    }

    Ok(())
}

pub fn cmd_use(version_str: &str) -> Result<()> {
    let version: KotlinVersion =
        version_str
            .parse()
            .map_err(|e| kargo_util::errors::KargoError::Toolchain {
                message: format!("Invalid version '{version_str}': {e}"),
            })?;

    if !install::is_installed(&version) {
        println!("  Kotlin {version} is not installed. Installing...");
        let config = GlobalConfig::load()?;
        let mirror = config.toolchain.kotlin_mirror.as_deref();
        install::install_kotlin(&version, mirror)?;
    }

    install::set_default(&version)?;
    println!("  Default Kotlin version set to {version}.");
    Ok(())
}

pub fn cmd_path() -> Result<PathBuf> {
    let config = GlobalConfig::load()?;
    let mirror = config.toolchain.kotlin_mirror.as_deref();

    let cwd = std::env::current_dir().map_err(kargo_util::errors::KargoError::Io)?;
    let paths = kargo_toolchain::discovery::resolve_project_toolchain(
        &cwd,
        config.toolchain.auto_download,
        mirror,
    )
    .or_else(|_| {
        let default =
            install::get_default().ok_or_else(|| kargo_util::errors::KargoError::Toolchain {
                message: "No Kargo project found and no default toolchain set".to_string(),
            })?;
        kargo_toolchain::discovery::resolve_toolchain(
            &default,
            config.toolchain.auto_download,
            mirror,
        )
    })?;

    Ok(paths.home)
}
