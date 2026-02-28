//! Artifact downloading from Maven repositories.

use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;

use crate::auth;
use crate::repository::MavenRepository;

const MAX_RETRIES: u32 = 3;
const RETRY_DELAY: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Build a shared reqwest client for Maven downloads.
pub fn build_client() -> miette::Result<Client> {
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent("kargo/0.1")
        .build()
        .map_err(|e| {
            kargo_util::errors::KargoError::Network {
                message: format!("Failed to create HTTP client: {e}"),
            }
            .into()
        })
}

/// Download raw bytes from a URL, with authentication and retries.
///
/// Returns `Ok(bytes)` on success, or an error after exhausting retries.
/// Returns `Ok(None)` for 404 (artifact not found in this repo).
pub async fn download_bytes(
    client: &Client,
    repo: &MavenRepository,
    url: &str,
) -> miette::Result<Option<Vec<u8>>> {
    let mut last_err = String::new();

    for attempt in 0..MAX_RETRIES {
        if attempt > 0 {
            tokio::time::sleep(RETRY_DELAY * attempt).await;
        }

        let mut req = client.get(url);
        req = auth::apply_auth(req, repo);

        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                if status == reqwest::StatusCode::NOT_FOUND {
                    return Ok(None);
                }
                if status.is_server_error() {
                    last_err = format!("HTTP {status} from {url}");
                    continue;
                }
                if !status.is_success() {
                    return Err(kargo_util::errors::KargoError::Network {
                        message: format!("HTTP {status} fetching {url}"),
                    }
                    .into());
                }

                let bytes =
                    resp.bytes()
                        .await
                        .map_err(|e| kargo_util::errors::KargoError::Network {
                            message: format!("Failed to read response from {url}: {e}"),
                        })?;
                return Ok(Some(bytes.to_vec()));
            }
            Err(e) if e.is_timeout() || e.is_connect() => {
                last_err = format!("{e}");
                continue;
            }
            Err(e) => {
                return Err(kargo_util::errors::KargoError::Network {
                    message: format!("Request to {url} failed: {e}"),
                }
                .into());
            }
        }
    }

    Err(kargo_util::errors::KargoError::Network {
        message: format!("Failed after {MAX_RETRIES} retries for {url}: {last_err}"),
    }
    .into())
}

/// Download an artifact (JAR, POM, etc.) with a progress bar.
pub async fn download_artifact(
    client: &Client,
    repo: &MavenRepository,
    url: &str,
    label: &str,
) -> miette::Result<Option<Vec<u8>>> {
    let mut req = client.get(url);
    req = auth::apply_auth(req, repo);

    let resp = req
        .send()
        .await
        .map_err(|e| kargo_util::errors::KargoError::Network {
            message: format!("Request to {url} failed: {e}"),
        })?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(kargo_util::errors::KargoError::Network {
            message: format!("HTTP {} fetching {url}", resp.status()),
        }
        .into());
    }

    let total = resp.content_length().unwrap_or(0);
    let pb = if total > 100_000 {
        let pb = ProgressBar::new(total);
        pb.set_style(
            ProgressStyle::with_template("  {msg} {bar:30.cyan/dim} {bytes}/{total_bytes}")
                .unwrap()
                .progress_chars("##-"),
        );
        pb.set_message(label.to_string());
        Some(pb)
    } else {
        None
    };

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| kargo_util::errors::KargoError::Network {
            message: format!("Failed to read {url}: {e}"),
        })?;

    if let Some(pb) = pb {
        pb.set_position(bytes.len() as u64);
        pb.finish_and_clear();
    }

    Ok(Some(bytes.to_vec()))
}

/// Download a text file (POM, metadata, checksum sidecar).
pub async fn download_text(
    client: &Client,
    repo: &MavenRepository,
    url: &str,
) -> miette::Result<Option<String>> {
    match download_bytes(client, repo, url).await? {
        Some(bytes) => Ok(Some(String::from_utf8_lossy(&bytes).to_string())),
        None => Ok(None),
    }
}
