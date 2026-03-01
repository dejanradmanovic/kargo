//! Android SDK discovery, installation, and component management.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use dialoguer::Select;
use kargo_util::errors::KargoError;

use crate::download;

/// Information about a discovered Android SDK.
#[derive(Debug, Clone)]
pub struct AndroidSdkInfo {
    pub home: PathBuf,
    pub has_cmdline_tools: bool,
    pub installed_platforms: Vec<u32>,
    pub installed_build_tools: Vec<String>,
}

/// Root for Kargo-managed Android SDK.
pub fn managed_android_sdk_dir() -> PathBuf {
    kargo_util::dirs_path().join("android-sdk")
}

/// Discover an installed Android SDK and inventory its components.
pub fn discover_android_sdk() -> Option<AndroidSdkInfo> {
    let candidates: Vec<PathBuf> = [
        std::env::var("ANDROID_HOME").ok().map(PathBuf::from),
        std::env::var("ANDROID_SDK_ROOT").ok().map(PathBuf::from),
        Some(managed_android_sdk_dir()),
        dirs_home().map(|h| h.join("Android/Sdk")),
        dirs_home().map(|h| h.join("Library/Android/sdk")),
    ]
    .into_iter()
    .flatten()
    .collect();

    for dir in candidates {
        if dir.is_dir() {
            return Some(inventory_android_sdk(&dir));
        }
    }
    None
}

/// Scan an Android SDK directory to find installed platforms and build-tools.
fn inventory_android_sdk(home: &Path) -> AndroidSdkInfo {
    let has_cmdline = home.join("cmdline-tools").is_dir();

    let mut installed_platforms: Vec<u32> = fs::read_dir(home.join("platforms"))
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let name = e.ok()?.file_name().to_string_lossy().to_string();
            name.strip_prefix("android-")?.parse().ok()
        })
        .collect();
    installed_platforms.sort();

    let mut installed_build_tools: Vec<String> = fs::read_dir(home.join("build-tools"))
        .into_iter()
        .flatten()
        .filter_map(|e| Some(e.ok()?.file_name().to_string_lossy().to_string()))
        .collect();
    installed_build_tools.sort();

    AndroidSdkInfo {
        home: home.to_path_buf(),
        has_cmdline_tools: has_cmdline,
        installed_platforms,
        installed_build_tools,
    }
}

/// Check if the Android SDK has the required compile-sdk platform installed.
pub fn has_platform(info: &AndroidSdkInfo, compile_sdk: u32) -> bool {
    info.installed_platforms.contains(&compile_sdk)
}

/// Ensure the required compile-sdk platform (and build-tools) are installed.
///
/// If the platform is missing and `sdkmanager` is available, installs it
/// automatically (accepting licenses). Returns an error only if installation
/// fails.
pub fn ensure_android_components(info: &AndroidSdkInfo, compile_sdk: u32) -> miette::Result<()> {
    let mut missing: Vec<String> = Vec::new();

    if !info.installed_platforms.contains(&compile_sdk) {
        missing.push(format!("platforms;android-{compile_sdk}"));
    }
    if info.installed_build_tools.is_empty() {
        missing.push("build-tools;35.0.0".to_string());
    }
    if !info.home.join("platform-tools").is_dir() {
        missing.push("platform-tools".to_string());
    }

    if missing.is_empty() {
        return Ok(());
    }

    let sdkmanager = sdkmanager_path(&info.home);
    if !sdkmanager.exists() {
        return Err(KargoError::Toolchain {
            message: format!(
                "Android SDK at {} is missing: {}.\n  \
                 sdkmanager not found — install components manually:\n    \
                 sdkmanager {}",
                info.home.display(),
                missing.join(", "),
                missing
                    .iter()
                    .map(|s| format!("\"{s}\""))
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
        }
        .into());
    }

    println!(
        "  Installing missing Android SDK components: {}",
        missing.join(", ")
    );

    accept_licenses(&sdkmanager, &info.home);

    let args: Vec<&str> = missing.iter().map(|s| s.as_str()).collect();
    let status = Command::new(&sdkmanager)
        .args(&args)
        .env("ANDROID_HOME", &info.home)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| KargoError::Toolchain {
            message: format!("sdkmanager failed: {e}"),
        })?;

    if !status.success() {
        return Err(KargoError::Toolchain {
            message: format!("sdkmanager failed to install: {}", missing.join(", ")),
        }
        .into());
    }

    println!("  Android SDK components installed.");
    Ok(())
}

/// Prompt the user for Android SDK installation when it's missing.
pub async fn prompt_and_install_android_sdk(compile_sdk: u32) -> miette::Result<AndroidSdkInfo> {
    let labels = [
        "Install automatically (recommended)",
        "Show me what to do manually",
    ];

    let selection = if atty::is(atty::Stream::Stdin) {
        Select::new()
            .with_prompt("  Android SDK not found. What would you like to do?")
            .items(&labels)
            .default(0)
            .interact()
            .map_err(|e| KargoError::Generic {
                message: format!("Prompt error: {e}"),
            })?
    } else {
        println!("  Android SDK not found (non-interactive, skipping).");
        return Err(KargoError::Toolchain {
            message: "Android SDK not found and running non-interactively".to_string(),
        }
        .into());
    };

    if selection == 1 {
        print_android_sdk_instructions(compile_sdk);
        return Err(KargoError::Toolchain {
            message: "Android SDK installation deferred to user".to_string(),
        }
        .into());
    }

    install_android_sdk(compile_sdk).await
}

/// Download command-line tools and use sdkmanager to install required components.
pub async fn install_android_sdk(compile_sdk: u32) -> miette::Result<AndroidSdkInfo> {
    let sdk_home = managed_android_sdk_dir();
    kargo_util::fs::ensure_dir(&sdk_home).map_err(KargoError::Io)?;

    println!("  Installing Android SDK...");

    let cmdline_url = android_cmdline_tools_url()?;
    let tmp_dir = tempfile::tempdir().map_err(KargoError::Io)?;
    let zip_path = tmp_dir.path().join("cmdline-tools.zip");
    download::download_file(&cmdline_url, &zip_path).await?;

    let cmdline_dest = sdk_home.join("cmdline-tools").join("latest");
    kargo_util::fs::ensure_dir(&cmdline_dest).map_err(KargoError::Io)?;
    super::extract_zip_to(&zip_path, &cmdline_dest)?;

    // The zip contains a `cmdline-tools/` wrapper dir — flatten it
    let inner = cmdline_dest.join("cmdline-tools");
    if inner.is_dir() {
        for entry in fs::read_dir(&inner).map_err(KargoError::Io)? {
            let entry = entry.map_err(KargoError::Io)?;
            let target = cmdline_dest.join(entry.file_name());
            if !target.exists() {
                fs::rename(entry.path(), target).map_err(KargoError::Io)?;
            }
        }
        if let Err(e) = fs::remove_dir_all(&inner) {
            tracing::warn!(
                "Failed to remove Android SDK cmdline-tools inner directory {}: {e}",
                inner.display()
            );
        }
    }

    let sdkmanager = sdkmanager_path(&sdk_home);
    if sdkmanager.exists() {
        println!("  Accepting Android SDK licenses...");
        accept_licenses(&sdkmanager, &sdk_home);

        println!("  Installing platform android-{compile_sdk}, build-tools, platform-tools...");
        let status = Command::new(&sdkmanager)
            .args([
                &format!("platforms;android-{compile_sdk}"),
                "build-tools;35.0.0",
                "platform-tools",
            ])
            .env("ANDROID_HOME", &sdk_home)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|e| KargoError::Toolchain {
                message: format!("sdkmanager failed: {e}"),
            })?;

        if !status.success() {
            println!("  Warning: sdkmanager exited with non-zero status");
        }
    } else {
        println!(
            "  Warning: sdkmanager not found at {}. Manual setup may be required.",
            sdkmanager.display()
        );
    }

    println!("  Android SDK installed at {}", sdk_home.display());
    Ok(inventory_android_sdk(&sdk_home))
}

// -----------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------

fn sdkmanager_path(sdk_home: &Path) -> PathBuf {
    let bin = if cfg!(windows) {
        "sdkmanager.bat"
    } else {
        "sdkmanager"
    };
    sdk_home
        .join("cmdline-tools")
        .join("latest")
        .join("bin")
        .join(bin)
}

fn accept_licenses(sdkmanager: &Path, sdk_home: &Path) {
    let _ = Command::new(sdkmanager)
        .arg("--licenses")
        .env("ANDROID_HOME", sdk_home)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut child| {
            if let Some(ref mut stdin) = child.stdin {
                for _ in 0..20 {
                    let _ = std::io::Write::write_all(stdin, b"y\n");
                }
            }
            child.wait()
        });
}

fn android_cmdline_tools_url() -> miette::Result<String> {
    let os = if cfg!(target_os = "macos") {
        "mac"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "win"
    } else {
        return Err(KargoError::Toolchain {
            message: "Unsupported OS for Android SDK download".to_string(),
        }
        .into());
    };

    Ok(format!(
        "https://dl.google.com/android/repository/commandlinetools-{os}-11076708_latest.zip"
    ))
}

fn print_android_sdk_instructions(compile_sdk: u32) {
    println!();
    println!("  To install the Android SDK manually:");
    println!();
    println!(
        "  1. Download command-line tools from https://developer.android.com/studio#command-tools"
    );
    println!("  2. Extract to a directory (e.g., ~/Android/Sdk/)");
    println!("  3. Set ANDROID_HOME to that directory");
    println!("  4. Run:");
    println!(
        "       sdkmanager \"platforms;android-{compile_sdk}\" \"build-tools;35.0.0\" \"platform-tools\""
    );
    println!();
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}
