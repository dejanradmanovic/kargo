//! Toolchain installation: extract, verify, and register compiler versions.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use kargo_util::errors::KargoError;

use crate::download;
use crate::version::KotlinVersion;

/// Root directory for managed toolchains: `~/.kargo/toolchains/`.
pub fn toolchains_dir() -> PathBuf {
    kargo_util::dirs_path().join("toolchains")
}

/// Directory for a specific Kotlin version.
pub fn toolchain_dir(version: &KotlinVersion) -> PathBuf {
    toolchains_dir().join(format!("kotlin-{version}"))
}

/// Path to the default-kotlin marker file.
fn default_marker_path() -> PathBuf {
    kargo_util::dirs_path().join("default-kotlin")
}

/// Check whether a specific Kotlin version is installed.
pub fn is_installed(version: &KotlinVersion) -> bool {
    toolchain_dir(version).is_dir()
}

/// Download, verify, extract, and register a Kotlin compiler version.
pub fn install_kotlin(version: &KotlinVersion, mirror: Option<&str>) -> miette::Result<PathBuf> {
    let dest = toolchain_dir(version);
    if dest.is_dir() {
        println!("  Kotlin {version} is already installed.");
        return Ok(dest);
    }

    println!("  Downloading Kotlin {version}...");

    let tmp_dir = tempfile::tempdir().map_err(KargoError::Io)?;
    let zip_path = tmp_dir
        .path()
        .join(format!("kotlin-compiler-{version}.zip"));

    let url = download::compiler_zip_url(version, mirror);
    download::download_file(&url, &zip_path)?;

    // Verify checksum
    match download::fetch_checksum(version, mirror) {
        Ok(expected) if !expected.is_empty() => {
            print!("  Verifying checksum... ");
            download::verify_checksum(&zip_path, &expected)?;
            println!("ok");
        }
        _ => {
            tracing::warn!("Checksum not available for Kotlin {version}, skipping verification");
        }
    }

    // Extract
    print!("  Extracting... ");
    kargo_util::fs::ensure_dir(&toolchains_dir()).map_err(KargoError::Io)?;
    extract_zip(&zip_path, &dest)?;
    println!("done");

    // The zip often contains a top-level `kotlinc/` directory.
    // If that's the only entry, move its contents up.
    flatten_single_child(&dest)?;

    println!("  Kotlin {version} installed to {}", dest.display());
    Ok(dest)
}

/// Remove an installed Kotlin toolchain.
pub fn uninstall_kotlin(version: &KotlinVersion) -> miette::Result<()> {
    let dir = toolchain_dir(version);
    if !dir.is_dir() {
        return Err(KargoError::Toolchain {
            message: format!("Kotlin {version} is not installed"),
        }
        .into());
    }
    fs::remove_dir_all(&dir).map_err(KargoError::Io)?;

    // Clear default if it pointed to this version
    if get_default().as_ref() == Some(version) {
        let _ = fs::remove_file(default_marker_path());
    }
    Ok(())
}

/// Set the default Kotlin version.
pub fn set_default(version: &KotlinVersion) -> miette::Result<()> {
    kargo_util::fs::ensure_dir(&kargo_util::dirs_path()).map_err(KargoError::Io)?;
    fs::write(default_marker_path(), version.to_string()).map_err(KargoError::Io)?;
    Ok(())
}

/// Read the default Kotlin version, if one is set.
pub fn get_default() -> Option<KotlinVersion> {
    fs::read_to_string(default_marker_path())
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

/// List all installed Kotlin versions, sorted ascending.
pub fn list_installed() -> Vec<KotlinVersion> {
    let dir = toolchains_dir();
    let mut versions: Vec<KotlinVersion> = fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let name = entry.ok()?.file_name().to_string_lossy().to_string();
            name.strip_prefix("kotlin-")?.parse().ok()
        })
        .collect();
    versions.sort();
    versions
}

/// Extract a zip archive to `dest`.
fn extract_zip(zip_path: &Path, dest: &Path) -> miette::Result<()> {
    let file = fs::File::open(zip_path).map_err(KargoError::Io)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| KargoError::Toolchain {
        message: format!("Failed to open zip: {e}"),
    })?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| KargoError::Toolchain {
            message: format!("Zip entry error: {e}"),
        })?;

        let out_path = dest.join(entry.mangled_name());

        if entry.is_dir() {
            fs::create_dir_all(&out_path).map_err(KargoError::Io)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(KargoError::Io)?;
            }
            let mut out = fs::File::create(&out_path).map_err(KargoError::Io)?;
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| KargoError::Toolchain {
                    message: format!("Failed to read zip entry: {e}"),
                })?;
            std::io::Write::write_all(&mut out, &buf).map_err(KargoError::Io)?;

            // Preserve executable bit on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = entry.unix_mode() {
                    fs::set_permissions(&out_path, fs::Permissions::from_mode(mode))
                        .map_err(KargoError::Io)?;
                }
            }
        }
    }
    Ok(())
}

/// If `dir` contains exactly one child directory, move its contents
/// up to `dir` (common for archives with a wrapper folder like `kotlinc/`).
fn flatten_single_child(dir: &Path) -> miette::Result<()> {
    let entries: Vec<_> = fs::read_dir(dir)
        .map_err(KargoError::Io)?
        .filter_map(|e| e.ok())
        .collect();

    if entries.len() == 1 && entries[0].path().is_dir() {
        let child = entries[0].path();
        let dir_name = dir
            .file_name()
            .unwrap_or(std::ffi::OsStr::new("archive"))
            .to_string_lossy();
        let tmp_name = dir.with_file_name(format!(".kargo-flatten-{dir_name}"));
        fs::rename(dir, &tmp_name).map_err(KargoError::Io)?;
        let child_name = match child.file_name() {
            Some(n) => n.to_os_string(),
            None => return Ok(()),
        };
        fs::rename(tmp_name.join(child_name), dir).map_err(KargoError::Io)?;
        fs::remove_dir_all(&tmp_name).map_err(KargoError::Io)?;
    }
    Ok(())
}
