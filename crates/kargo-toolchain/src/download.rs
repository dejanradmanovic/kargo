//! Kotlin compiler download from GitHub releases or configurable mirrors.

use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};

use crate::version::KotlinVersion;

const KOTLIN_RELEASE_BASE: &str = "https://github.com/JetBrains/kotlin/releases/download";

/// Build the download URL for a Kotlin compiler zip.
pub fn compiler_zip_url(version: &KotlinVersion, mirror: Option<&str>) -> String {
    let base = mirror.unwrap_or(KOTLIN_RELEASE_BASE);
    format!(
        "{}/v{}/kotlin-compiler-{}.zip",
        base.trim_end_matches('/'),
        version,
        version
    )
}

/// Build the URL for the sha256 checksum file.
pub fn checksum_url(version: &KotlinVersion, mirror: Option<&str>) -> String {
    format!("{}.sha256", compiler_zip_url(version, mirror))
}

/// Download a file from `url` to `dest`, showing a progress bar.
/// Returns the path written.
pub fn download_file(url: &str, dest: &Path) -> miette::Result<PathBuf> {
    tracing::info!("Downloading {}", url);

    let resp = reqwest::blocking::Client::builder()
        .user_agent("kargo")
        .build()
        .map_err(|e| kargo_util::errors::KargoError::Network {
            message: format!("Failed to build HTTP client: {e}"),
        })?
        .get(url)
        .send()
        .map_err(|e| kargo_util::errors::KargoError::Network {
            message: format!("Failed to download {url}: {e}"),
        })?;

    if !resp.status().is_success() {
        return Err(kargo_util::errors::KargoError::Network {
            message: format!("HTTP {} for {url}", resp.status()),
        }
        .into());
    }

    let total = resp.content_length().unwrap_or(0);
    let pb = if total > 0 {
        let pb = ProgressBar::new(total);
        pb.set_style(
            ProgressStyle::with_template("  {bar:40.cyan/dim} {bytes}/{total_bytes} ({eta})")
                .unwrap()
                .progress_chars("##-"),
        );
        Some(pb)
    } else {
        None
    };

    let mut out = File::create(dest).map_err(kargo_util::errors::KargoError::Io)?;
    let mut reader = resp;
    let mut buf = [0u8; 8192];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| kargo_util::errors::KargoError::Network {
                message: format!("Read error: {e}"),
            })?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])
            .map_err(kargo_util::errors::KargoError::Io)?;
        if let Some(ref pb) = pb {
            pb.inc(n as u64);
        }
    }

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    Ok(dest.to_path_buf())
}

/// Fetch the expected sha256 checksum string from the remote `.sha256` file.
pub fn fetch_checksum(version: &KotlinVersion, mirror: Option<&str>) -> miette::Result<String> {
    let url = checksum_url(version, mirror);
    let body = reqwest::blocking::Client::builder()
        .user_agent("kargo")
        .build()
        .map_err(|e| kargo_util::errors::KargoError::Network {
            message: format!("HTTP client error: {e}"),
        })?
        .get(&url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.text())
        .map_err(|e| kargo_util::errors::KargoError::Network {
            message: format!("Failed to fetch checksum from {url}: {e}"),
        })?;

    // The file may contain "hash  filename" or just the hash
    Ok(body.split_whitespace().next().unwrap_or("").to_string())
}

/// Compute SHA-256 of a file and compare to the expected hex digest.
pub fn verify_checksum(file: &Path, expected: &str) -> miette::Result<()> {
    let mut f = File::open(file).map_err(kargo_util::errors::KargoError::Io)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(kargo_util::errors::KargoError::Io)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let actual = format!("{:x}", hasher.finalize());
    if actual != expected {
        return Err(kargo_util::errors::KargoError::Toolchain {
            message: format!(
                "Checksum mismatch for {}: expected {expected}, got {actual}",
                file.display()
            ),
        }
        .into());
    }
    Ok(())
}
