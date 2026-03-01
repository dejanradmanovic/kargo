//! Kotlin compiler download from GitHub releases or configurable mirrors.

use std::path::{Path, PathBuf};

use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

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
pub async fn download_file(url: &str, dest: &Path) -> miette::Result<PathBuf> {
    tracing::info!("Downloading {}", url);

    let client = reqwest::Client::builder()
        .user_agent("kargo")
        .build()
        .map_err(|e| kargo_util::errors::KargoError::Network {
            message: format!("Failed to build HTTP client: {e}"),
        })?;

    let resp =
        client
            .get(url)
            .send()
            .await
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

    let mut out = tokio::fs::File::create(dest)
        .await
        .map_err(kargo_util::errors::KargoError::Io)?;
    let mut stream = resp.bytes_stream();

    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| kargo_util::errors::KargoError::Network {
            message: format!("Read error: {e}"),
        })?;
        out.write_all(&chunk)
            .await
            .map_err(kargo_util::errors::KargoError::Io)?;
        if let Some(ref pb) = pb {
            pb.inc(chunk.len() as u64);
        }
    }

    out.flush()
        .await
        .map_err(kargo_util::errors::KargoError::Io)?;

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    Ok(dest.to_path_buf())
}

/// Fetch the expected sha256 checksum string from the remote `.sha256` file.
pub async fn fetch_checksum(
    version: &KotlinVersion,
    mirror: Option<&str>,
) -> miette::Result<String> {
    let url = checksum_url(version, mirror);
    let body = reqwest::Client::builder()
        .user_agent("kargo")
        .build()
        .map_err(|e| kargo_util::errors::KargoError::Network {
            message: format!("HTTP client error: {e}"),
        })?
        .get(&url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| kargo_util::errors::KargoError::Network {
            message: format!("Failed to fetch checksum from {url}: {e}"),
        })?
        .text()
        .await
        .map_err(|e| kargo_util::errors::KargoError::Network {
            message: format!("Failed to read checksum body from {url}: {e}"),
        })?;

    Ok(body.split_whitespace().next().unwrap_or("").to_string())
}

/// Compute SHA-256 of a file and compare to the expected hex digest.
pub fn verify_checksum(file: &Path, expected: &str) -> miette::Result<()> {
    let mut f = std::fs::File::open(file).map_err(kargo_util::errors::KargoError::Io)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 8192];
    loop {
        let n =
            std::io::Read::read(&mut f, &mut buf).map_err(kargo_util::errors::KargoError::Io)?;
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
