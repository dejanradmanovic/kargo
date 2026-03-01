//! Operation: scan resolved dependencies for known vulnerabilities via OSV.

use std::path::Path;

use kargo_core::lockfile::Lockfile;
use kargo_core::manifest::Manifest;
use kargo_maven::cache::LocalCache;
use kargo_maven::download;
use kargo_resolver::resolver;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// Options for `kargo audit`.
#[derive(Default)]
pub struct AuditOptions {
    /// Minimum severity to report: "low", "moderate", "high", "critical".
    pub fail_on: Option<String>,
    /// CVE IDs to ignore (from `[audit] ignore` in Kargo.toml).
    pub ignore: Vec<String>,
}

/// A single vulnerability finding.
pub struct Finding {
    pub id: String,
    pub summary: String,
    pub severity: String,
    pub group: String,
    pub artifact: String,
    pub version: String,
    pub fixed: Option<String>,
    pub url: String,
}

/// Query payload for the OSV batch API.
#[derive(Serialize)]
struct OsvBatchQuery {
    queries: Vec<OsvQuery>,
}

#[derive(Serialize)]
struct OsvQuery {
    package: OsvPackage,
    version: String,
}

#[derive(Serialize)]
struct OsvPackage {
    name: String,
    ecosystem: String,
}

/// Response from OSV batch API.
#[derive(Deserialize)]
struct OsvBatchResponse {
    results: Vec<OsvQueryResult>,
}

#[derive(Deserialize)]
struct OsvQueryResult {
    #[serde(default)]
    vulns: Vec<OsvVuln>,
}

#[derive(Deserialize)]
struct OsvVuln {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    severity: Vec<OsvSeverity>,
    #[serde(default)]
    affected: Vec<OsvAffected>,
    #[serde(default)]
    references: Vec<OsvReference>,
}

#[derive(Deserialize)]
struct OsvSeverity {
    #[serde(rename = "type")]
    severity_type: String,
    score: String,
}

#[derive(Deserialize)]
struct OsvAffected {
    #[serde(default)]
    ranges: Vec<OsvRange>,
}

#[derive(Deserialize)]
struct OsvRange {
    #[serde(default)]
    events: Vec<OsvEvent>,
}

#[derive(Deserialize)]
struct OsvEvent {
    #[serde(default)]
    fixed: Option<String>,
}

#[derive(Deserialize)]
struct OsvReference {
    #[serde(rename = "type")]
    ref_type: String,
    url: String,
}

const OSV_BATCH_URL: &str = "https://api.osv.dev/v1/querybatch";
const BATCH_SIZE: usize = 1000;

/// Scan all resolved dependencies for known vulnerabilities.
pub async fn audit(project_root: &Path, opts: &AuditOptions) -> miette::Result<()> {
    let manifest_path = project_root.join("Kargo.toml");
    let manifest = Manifest::from_path(&manifest_path)?;
    let repos = resolver::build_repos(&manifest);
    let cache = LocalCache::new(project_root);

    let lockfile_path = project_root.join("Kargo.lock");
    let existing_lock = if lockfile_path.is_file() {
        Lockfile::from_path(&lockfile_path).ok()
    } else {
        None
    };

    let sp = kargo_util::progress::spinner("Resolving dependencies...");
    let client = download::build_client()?;
    let result =
        resolver::resolve(&manifest, &repos, &cache, existing_lock.as_ref(), &client).await?;
    sp.finish_and_clear();

    let dep_count = result.artifacts.len();
    let sp = kargo_util::progress::spinner(&format!(
        "Scanning {dep_count} dependencies for vulnerabilities..."
    ));

    let mut all_findings: Vec<Finding> = Vec::new();

    // Build queries in batches
    for chunk in result.artifacts.chunks(BATCH_SIZE) {
        let queries: Vec<OsvQuery> = chunk
            .iter()
            .map(|a| OsvQuery {
                package: OsvPackage {
                    name: format!("{}:{}", a.group, a.artifact),
                    ecosystem: "Maven".to_string(),
                },
                version: a.version.clone(),
            })
            .collect();

        let batch = OsvBatchQuery { queries };
        let findings = query_osv_batch(&client, &batch, chunk).await?;
        all_findings.extend(findings);
    }

    // Apply ignores
    all_findings.retain(|f| !opts.ignore.contains(&f.id));

    sp.finish_and_clear();

    if all_findings.is_empty() {
        kargo_util::progress::status("Audit", &format!("{dep_count} dependencies — no vulnerabilities found"));
        return Ok(());
    }

    // Sort by severity (critical > high > moderate > low)
    all_findings.sort_by(|a, b| severity_rank(&b.severity).cmp(&severity_rank(&a.severity)));

    // Print report
    println!();
    println!(
        "Found {} vulnerabilities in {dep_count} dependencies:",
        all_findings.len()
    );
    println!();

    for f in &all_findings {
        let sev_display = if f.severity.is_empty() {
            "UNKNOWN".to_string()
        } else {
            f.severity.to_uppercase()
        };
        let fix = f
            .fixed
            .as_deref()
            .map(|v| format!(" (fix: >= {v})"))
            .unwrap_or_default();

        println!("  [{sev_display}] {}", f.id);
        println!("    {}:{}:{}{fix}", f.group, f.artifact, f.version);
        if !f.summary.is_empty() {
            println!("    {}", f.summary);
        }
        if !f.url.is_empty() {
            println!("    {}", f.url);
        }
        println!();
    }

    // Fail based on severity threshold
    if let Some(ref threshold) = opts.fail_on {
        let threshold_rank = severity_rank(threshold);
        let has_severe = all_findings
            .iter()
            .any(|f| severity_rank(&f.severity) >= threshold_rank);
        if has_severe {
            return Err(kargo_util::errors::KargoError::Generic {
                message: format!(
                    "Audit failed: found vulnerabilities at or above '{}' severity",
                    threshold
                ),
            }
            .into());
        }
    }

    Ok(())
}

async fn query_osv_batch(
    client: &Client,
    batch: &OsvBatchQuery,
    artifacts: &[resolver::ResolvedArtifact],
) -> miette::Result<Vec<Finding>> {
    let resp = client
        .post(OSV_BATCH_URL)
        .json(batch)
        .send()
        .await
        .map_err(|e| kargo_util::errors::KargoError::Network {
            message: format!("OSV API request failed: {e}"),
        })?;

    if !resp.status().is_success() {
        return Err(kargo_util::errors::KargoError::Network {
            message: format!("OSV API returned HTTP {}", resp.status()),
        }
        .into());
    }

    let body: OsvBatchResponse =
        resp.json()
            .await
            .map_err(|e| kargo_util::errors::KargoError::Network {
                message: format!("Failed to parse OSV response: {e}"),
            })?;

    let mut findings = Vec::new();

    for (i, query_result) in body.results.iter().enumerate() {
        let artifact = match artifacts.get(i) {
            Some(a) => a,
            None => continue,
        };

        for vuln in &query_result.vulns {
            let severity = extract_severity(vuln);
            let fixed = extract_fixed_version(vuln);
            let url = vuln
                .references
                .iter()
                .find(|r| r.ref_type == "ADVISORY" || r.ref_type == "WEB")
                .map(|r| r.url.clone())
                .unwrap_or_else(|| format!("https://osv.dev/vulnerability/{}", vuln.id));

            findings.push(Finding {
                id: vuln.id.clone(),
                summary: vuln.summary.clone(),
                severity,
                group: artifact.group.clone(),
                artifact: artifact.artifact.clone(),
                version: artifact.version.clone(),
                fixed,
                url,
            });
        }
    }

    Ok(findings)
}

fn extract_severity(vuln: &OsvVuln) -> String {
    for sev in &vuln.severity {
        if sev.severity_type == "CVSS_V3" {
            let score: f64 = sev
                .score
                .split('/')
                .next()
                .and_then(|s| {
                    // CVSS v3 vector string: extract base score
                    if s.starts_with("CVSS:") {
                        None
                    } else {
                        s.parse().ok()
                    }
                })
                .unwrap_or(0.0);

            if score == 0.0 {
                // Try to extract from the full vector string
                return cvss_to_severity(&sev.score);
            }

            return match score {
                s if s >= 9.0 => "critical".to_string(),
                s if s >= 7.0 => "high".to_string(),
                s if s >= 4.0 => "moderate".to_string(),
                _ => "low".to_string(),
            };
        }
    }

    "unknown".to_string()
}

fn cvss_to_severity(vector: &str) -> String {
    // OSV often stores the full CVSS vector, not the score.
    // We do a rough mapping based on the attack complexity.
    if vector.contains("/AV:N") && vector.contains("/AC:L") && vector.contains("/PR:N") {
        "high".to_string()
    } else if vector.contains("/AV:N") {
        "moderate".to_string()
    } else {
        "low".to_string()
    }
}

fn extract_fixed_version(vuln: &OsvVuln) -> Option<String> {
    for affected in &vuln.affected {
        for range in &affected.ranges {
            for event in &range.events {
                if let Some(ref fixed) = event.fixed {
                    return Some(fixed.clone());
                }
            }
        }
    }
    None
}

fn severity_rank(severity: &str) -> u8 {
    match severity.to_lowercase().as_str() {
        "critical" => 4,
        "high" => 3,
        "moderate" | "medium" => 2,
        "low" => 1,
        _ => 0,
    }
}
