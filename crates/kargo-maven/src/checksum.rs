//! Artifact checksum verification (SHA-1, SHA-256, MD5).

use md5::Md5;
use reqwest::Client;
use sha1::Sha1;
use sha2::{Digest, Sha256};

use crate::download;
use crate::repository::MavenRepository;

/// Verify the checksum of downloaded data against sidecar files from the repo.
///
/// Tries SHA-256 first, falls back to SHA-1, then MD5.
/// Logs a warning if no checksum sidecar is available.
pub async fn verify(
    client: &Client,
    repo: &MavenRepository,
    file_url: &str,
    data: &[u8],
) -> miette::Result<()> {
    // Try SHA-256
    let sha256_url = format!("{file_url}.sha256");
    if let Some(expected) = download::download_text(client, repo, &sha256_url).await? {
        let expected = extract_hash(&expected);
        let actual = hex_sha256(data);
        return check(&actual, &expected, "SHA-256", file_url);
    }

    // Try SHA-1
    let sha1_url = format!("{file_url}.sha1");
    if let Some(expected) = download::download_text(client, repo, &sha1_url).await? {
        let expected = extract_hash(&expected);
        let actual = hex_sha1(data);
        return check(&actual, &expected, "SHA-1", file_url);
    }

    // Try MD5
    let md5_url = format!("{file_url}.md5");
    if let Some(expected) = download::download_text(client, repo, &md5_url).await? {
        let expected = extract_hash(&expected);
        let actual = hex_md5(data);
        return check(&actual, &expected, "MD5", file_url);
    }

    tracing::warn!("No checksum sidecar found for {file_url}");
    Ok(())
}

fn check(actual: &str, expected: &str, algo: &str, url: &str) -> miette::Result<()> {
    if actual.eq_ignore_ascii_case(expected) {
        tracing::debug!("{algo} ok for {url}");
        Ok(())
    } else {
        Err(kargo_util::errors::KargoError::Network {
            message: format!("{algo} mismatch for {url}: expected {expected}, got {actual}"),
        }
        .into())
    }
}

/// Extract the hex hash from a checksum file.
///
/// Maven checksum files may contain just the hash, or `hash  filename`.
fn extract_hash(content: &str) -> String {
    content.split_whitespace().next().unwrap_or("").to_string()
}

fn hex_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

fn hex_sha1(data: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

fn hex_md5(data: &[u8]) -> String {
    let mut hasher = Md5::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_hash_simple() {
        assert_eq!(extract_hash("abc123\n"), "abc123");
    }

    #[test]
    fn extract_hash_with_filename() {
        assert_eq!(extract_hash("abc123  my-lib-1.0.jar\n"), "abc123");
    }

    #[test]
    fn sha256_computation() {
        let hash = hex_sha256(b"hello world");
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn md5_computation() {
        let hash = hex_md5(b"hello world");
        assert_eq!(hash, "5eb63bbbe01eeed093cb22bb8f5acdc3");
    }
}
