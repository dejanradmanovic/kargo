//! Self-update via GitHub Releases.
//!
//! Checks the latest release tag, compares with the running version, and
//! downloads + replaces the binary if a newer version is available.

use std::fs;
use std::path::{Path, PathBuf};

use semver::Version;
use serde::Deserialize;

use kargo_util::errors::KargoError;

const GITHUB_REPO: &str = "dejanradmanovic/kargo";
const GITHUB_API_BASE: &str = "https://api.github.com";

// -----------------------------------------------------------------------
// GitHub API types
// -----------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct GhRelease {
    tag_name: String,
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

// -----------------------------------------------------------------------
// Public types
// -----------------------------------------------------------------------

/// Information about an available update.
#[derive(Debug)]
pub struct UpdateInfo {
    pub current: Version,
    pub latest: Version,
    pub asset_url: String,
    pub asset_name: String,
}

/// Result of a version check.
#[derive(Debug)]
pub enum UpdateCheck {
    /// A newer version is available.
    Available(UpdateInfo),
    /// Already on the latest (or newer) version.
    UpToDate(Version),
}

// -----------------------------------------------------------------------
// Public API
// -----------------------------------------------------------------------

/// Query GitHub Releases for the latest version and compare with `current`.
pub async fn check_for_update(current_version: &str) -> miette::Result<UpdateCheck> {
    let current: Version = current_version
        .trim_start_matches('v')
        .parse()
        .map_err(|e| KargoError::Generic {
            message: format!("Cannot parse current version '{current_version}': {e}"),
        })?;

    let release = fetch_latest_release().await?;
    let latest: Version = release
        .tag_name
        .trim_start_matches('v')
        .parse()
        .map_err(|e| KargoError::Generic {
            message: format!("Cannot parse release tag '{}': {e}", release.tag_name),
        })?;

    if current >= latest {
        return Ok(UpdateCheck::UpToDate(current));
    }

    let expected = platform_asset_name().ok_or_else(|| KargoError::Toolchain {
        message: "Unsupported OS/architecture for self-update".to_string(),
    })?;

    let asset = release
        .assets
        .iter()
        .find(|a| a.name == expected)
        .ok_or_else(|| KargoError::Toolchain {
            message: format!(
                "Release {} has no asset for this platform (expected '{expected}').\n  \
                 Available assets: {}",
                release.tag_name,
                release
                    .assets
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        })?;

    Ok(UpdateCheck::Available(UpdateInfo {
        current,
        latest,
        asset_url: asset.browser_download_url.clone(),
        asset_name: asset.name.clone(),
    }))
}

/// Download and install the update, replacing the currently running binary.
pub async fn apply_update(info: &UpdateInfo) -> miette::Result<()> {
    let current_exe = std::env::current_exe().map_err(KargoError::Io)?;

    let tmp_dir = tempfile::tempdir().map_err(KargoError::Io)?;
    let archive_path = tmp_dir.path().join(&info.asset_name);

    println!("  Downloading Kargo {}...", info.latest);
    kargo_toolchain::download::download_file(&info.asset_url, &archive_path).await?;

    let checksum_url = format!("{}.sha256", info.asset_url);
    match reqwest::get(&checksum_url).await {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(expected) = resp.text().await {
                let expected = expected.split_whitespace().next().unwrap_or("").to_string();
                if !expected.is_empty() {
                    let actual = kargo_util::hash::sha256_file(&archive_path)?;
                    if actual != expected {
                        return Err(KargoError::Generic {
                            message: format!(
                                "Checksum mismatch for downloaded update: expected {expected}, got {actual}"
                            ),
                        }
                        .into());
                    }
                }
            }
        }
        _ => {
            tracing::warn!("Could not verify checksum for downloaded update");
        }
    }

    let new_binary = extract_binary(&archive_path, tmp_dir.path())?;

    replace_binary(&new_binary, &current_exe)?;

    println!("  Updated: {} -> {}", info.current, info.latest);
    Ok(())
}

// -----------------------------------------------------------------------
// Internals
// -----------------------------------------------------------------------

async fn fetch_latest_release() -> miette::Result<GhRelease> {
    let url = format!("{}/repos/{}/releases/latest", GITHUB_API_BASE, GITHUB_REPO);

    let client = reqwest::Client::builder()
        .user_agent("kargo-self-update")
        .build()
        .map_err(|e| KargoError::Network {
            message: format!("HTTP client error: {e}"),
        })?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| KargoError::Network {
            message: format!("Failed to reach GitHub: {e}"),
        })?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(KargoError::Network {
            message: format!(
                "No releases found for {GITHUB_REPO}.\n  \
                 Create a release at https://github.com/{GITHUB_REPO}/releases"
            ),
        }
        .into());
    }

    if !resp.status().is_success() {
        return Err(KargoError::Network {
            message: format!("GitHub API returned HTTP {} for {url}", resp.status()),
        }
        .into());
    }

    resp.json::<GhRelease>().await.map_err(|e| {
        KargoError::Network {
            message: format!("Failed to parse GitHub release JSON: {e}"),
        }
        .into()
    })
}

/// Expected asset filename for the current platform.
///
/// Convention: `kargo-<arch>-<target_triple_suffix>.<ext>`
///
/// Examples:
///   - `kargo-aarch64-apple-darwin.tar.gz`
///   - `kargo-x86_64-unknown-linux-gnu.tar.gz`
///   - `kargo-x86_64-pc-windows-msvc.zip`
fn platform_asset_name() -> Option<String> {
    let os = if cfg!(target_os = "macos") {
        "apple-darwin"
    } else if cfg!(target_os = "linux") {
        "unknown-linux-gnu"
    } else if cfg!(target_os = "windows") {
        "pc-windows-msvc"
    } else {
        return None;
    };

    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        return None;
    };

    let ext = if cfg!(target_os = "windows") {
        "zip"
    } else {
        "tar.gz"
    };

    Some(format!("kargo-{arch}-{os}.{ext}"))
}

/// Extract the `kargo` binary from the downloaded archive.
fn extract_binary(archive: &Path, dest_dir: &Path) -> miette::Result<PathBuf> {
    let name = archive.file_name().unwrap_or_default().to_string_lossy();

    if name.ends_with(".zip") {
        extract_binary_from_zip(archive, dest_dir)
    } else {
        extract_binary_from_tarball(archive, dest_dir)
    }
}

fn extract_binary_from_tarball(archive: &Path, dest_dir: &Path) -> miette::Result<PathBuf> {
    let status = std::process::Command::new("tar")
        .args(["xzf", &archive.to_string_lossy(), "-C"])
        .arg(dest_dir)
        .status()
        .map_err(|e| KargoError::Toolchain {
            message: format!("Failed to run tar: {e}"),
        })?;

    if !status.success() {
        return Err(KargoError::Toolchain {
            message: format!("tar extraction failed for {}", archive.display()),
        }
        .into());
    }

    find_kargo_binary(dest_dir)
}

fn extract_binary_from_zip(archive: &Path, dest_dir: &Path) -> miette::Result<PathBuf> {
    let file = fs::File::open(archive).map_err(KargoError::Io)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| KargoError::Toolchain {
        message: format!("Failed to open zip: {e}"),
    })?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| KargoError::Toolchain {
            message: format!("Zip entry error: {e}"),
        })?;
        let out_path = dest_dir.join(entry.mangled_name());
        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(KargoError::Io)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(KargoError::Io)?;
            }
            let mut out = fs::File::create(&out_path).map_err(KargoError::Io)?;
            std::io::copy(&mut entry, &mut out).map_err(|e| KargoError::Toolchain {
                message: format!("Extract error: {e}"),
            })?;
        }
    }

    find_kargo_binary(dest_dir)
}

/// Locate the `kargo` binary inside the extracted directory.
fn find_kargo_binary(dir: &Path) -> miette::Result<PathBuf> {
    let bin_name = if cfg!(target_os = "windows") {
        "kargo.exe"
    } else {
        "kargo"
    };

    let direct = dir.join(bin_name);
    if direct.is_file() {
        return Ok(direct);
    }

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let candidate = entry.path().join(bin_name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    Err(KargoError::Toolchain {
        message: format!(
            "Could not find '{bin_name}' in the downloaded archive.\n  \
             Contents of {}:\n  {}",
            dir.display(),
            ls_dir(dir),
        ),
    }
    .into())
}

fn ls_dir(dir: &Path) -> String {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Replace the running binary with the new one.
fn replace_binary(new_binary: &Path, current_exe: &Path) -> miette::Result<()> {
    let backup = current_exe.with_extension("old");

    if let Err(e) = fs::remove_file(&backup) {
        tracing::warn!("Failed to remove old backup file {}: {e}", backup.display());
    }

    fs::rename(current_exe, &backup).map_err(|e| KargoError::Toolchain {
        message: format!(
            "Cannot rename current binary to {}: {e}\n  \
             You may need to run with elevated permissions.",
            backup.display()
        ),
    })?;

    fs::copy(new_binary, current_exe).map_err(|e| {
        if let Err(revert_err) = fs::rename(&backup, current_exe) {
            tracing::warn!(
                "Failed to restore backup {}: {revert_err}",
                backup.display()
            );
        }
        KargoError::Toolchain {
            message: format!(
                "Failed to install new binary to {}: {e}",
                current_exe.display()
            ),
        }
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = fs::set_permissions(current_exe, fs::Permissions::from_mode(0o755)) {
            tracing::warn!(
                "Failed to set permissions on {}: {e}",
                current_exe.display()
            );
        }
    }

    if let Err(e) = fs::remove_file(&backup) {
        tracing::warn!("Failed to remove backup file {}: {e}", backup.display());
    }

    Ok(())
}
