//! JDK discovery, installation, and management.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use dialoguer::Select;
use kargo_util::errors::KargoError;

use crate::download;

/// Information about a discovered JDK.
#[derive(Debug, Clone)]
pub struct JdkInfo {
    pub home: PathBuf,
    pub version: String,
}

/// JDK distributions Kargo can install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JdkDistribution {
    Temurin,
    Corretto,
    Zulu,
}

impl fmt::Display for JdkDistribution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Temurin => write!(f, "Eclipse Temurin"),
            Self::Corretto => write!(f, "Amazon Corretto"),
            Self::Zulu => write!(f, "Azul Zulu"),
        }
    }
}

/// Root directory for Kargo-managed JDKs.
pub fn jdks_dir() -> PathBuf {
    kargo_util::dirs_path().join("jdks")
}

/// Discover an installed JDK by checking common locations.
///
/// Search order: explicit config path -> `JAVA_HOME` env -> managed JDKs
/// -> common OS paths.
pub fn discover_jdk(config_jdk: Option<&str>) -> Option<JdkInfo> {
    if let Some(path) = config_jdk {
        if let Some(info) = validate_jdk(&PathBuf::from(path)) {
            return Some(info);
        }
    }

    if let Ok(home) = std::env::var("JAVA_HOME") {
        if let Some(info) = validate_jdk(&PathBuf::from(&home)) {
            return Some(info);
        }
    }

    if let Some(info) = discover_managed_jdk(None) {
        return Some(info);
    }

    for candidate in common_jdk_paths() {
        if let Some(info) = validate_jdk(&candidate) {
            return Some(info);
        }
    }

    None
}

/// Discover a JDK whose major version is >= `required_major`.
///
/// Searches the same locations as [`discover_jdk`] but filters out JDKs
/// whose version is too low. Returns `None` if no compatible JDK is found.
pub fn discover_jdk_for_target(config_jdk: Option<&str>, required_major: u32) -> Option<JdkInfo> {
    let accept = |info: &JdkInfo| jdk_major(&info.version) >= required_major;

    if let Some(path) = config_jdk {
        if let Some(info) = validate_jdk(&PathBuf::from(path)) {
            if accept(&info) {
                return Some(info);
            }
        }
    }

    if let Ok(home) = std::env::var("JAVA_HOME") {
        if let Some(info) = validate_jdk(&PathBuf::from(&home)) {
            if accept(&info) {
                return Some(info);
            }
        }
    }

    if let Some(info) = discover_managed_jdk(Some(required_major)) {
        return Some(info);
    }

    for candidate in common_jdk_paths() {
        if let Some(info) = validate_jdk(&candidate) {
            if accept(&info) {
                return Some(info);
            }
        }
    }

    None
}

/// Parse the major version number from a JDK version string (e.g., "21" -> 21).
pub fn jdk_major(version: &str) -> u32 {
    version.parse().unwrap_or(0)
}

/// Look for a JDK in `~/.kargo/jdks/`, preferring the highest version.
/// If `min_major` is provided, only JDKs with version >= that value are returned.
fn discover_managed_jdk(min_major: Option<u32>) -> Option<JdkInfo> {
    let dir = jdks_dir();
    let mut found: Vec<(PathBuf, JdkInfo)> = fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            let p = entry.path();
            validate_jdk(&p).map(|info| (p, info))
        })
        .filter(|(_, info)| min_major.map_or(true, |min| jdk_major(&info.version) >= min))
        .collect();
    found.sort_by(|a, b| jdk_major(&b.1.version).cmp(&jdk_major(&a.1.version)));
    found.into_iter().next().map(|(_, info)| info)
}

/// Validate a JDK home directory by running `java -version`.
pub fn validate_jdk(home: &Path) -> Option<JdkInfo> {
    let java = if cfg!(windows) {
        home.join("bin").join("java.exe")
    } else {
        home.join("bin").join("java")
    };
    if !java.exists() {
        return None;
    }

    let output = Command::new(&java).arg("-version").output().ok()?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let version = parse_java_version(&stderr)?;

    Some(JdkInfo {
        home: home.to_path_buf(),
        version,
    })
}

/// Parse a version string from `java -version` stderr output.
/// Example: `openjdk version "21.0.2" 2024-01-16` -> "21"
fn parse_java_version(output: &str) -> Option<String> {
    for line in output.lines() {
        if let Some(start) = line.find('"') {
            if let Some(end) = line[start + 1..].find('"') {
                let full = &line[start + 1..start + 1 + end];
                let major = if full.starts_with("1.") {
                    full.split('.').nth(1).unwrap_or(full)
                } else {
                    full.split('.').next().unwrap_or(full)
                };
                return Some(major.to_string());
            }
        }
    }
    None
}

fn common_jdk_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    #[cfg(target_os = "macos")]
    {
        let lib_jvm = PathBuf::from("/Library/Java/JavaVirtualMachines");
        if let Ok(entries) = fs::read_dir(&lib_jvm) {
            for entry in entries.filter_map(|e| e.ok()) {
                let contents = entry.path().join("Contents/Home");
                if contents.is_dir() {
                    paths.push(contents);
                }
            }
        }
        paths.push(PathBuf::from(
            "/opt/homebrew/opt/openjdk/libexec/openjdk.jdk/Contents/Home",
        ));
        paths.push(PathBuf::from(
            "/usr/local/opt/openjdk/libexec/openjdk.jdk/Contents/Home",
        ));
    }

    #[cfg(target_os = "linux")]
    {
        let jvm_dir = PathBuf::from("/usr/lib/jvm");
        if let Ok(entries) = fs::read_dir(&jvm_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                paths.push(entry.path());
            }
        }
        paths.push(PathBuf::from("/usr/local/java"));
    }

    #[cfg(target_os = "windows")]
    {
        for base in &[
            "C:\\Program Files\\Java",
            "C:\\Program Files\\Eclipse Adoptium",
        ] {
            let dir = PathBuf::from(base);
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    paths.push(entry.path());
                }
            }
        }
    }

    paths
}

/// Prompt the user to choose a JDK distribution and install it.
pub fn prompt_and_install_jdk(java_target: &str) -> miette::Result<JdkInfo> {
    let distributions = [
        JdkDistribution::Temurin,
        JdkDistribution::Corretto,
        JdkDistribution::Zulu,
    ];

    let labels: Vec<String> = vec![
        "Eclipse Temurin (recommended)".to_string(),
        "Amazon Corretto".to_string(),
        "Azul Zulu".to_string(),
    ];

    let selection = if atty::is(atty::Stream::Stdin) {
        Select::new()
            .with_prompt(format!(
                "  JDK not found. Which distribution would you like to install? (JDK {java_target})"
            ))
            .items(&labels)
            .default(0)
            .interact()
            .map_err(|e| KargoError::Generic {
                message: format!("Prompt error: {e}"),
            })?
    } else {
        println!("  JDK not found (non-interactive, using Temurin).");
        0
    };

    let dist = distributions[selection];
    install_jdk(java_target, dist)
}

/// Download and install a JDK into `~/.kargo/jdks/<dist>-<version>/`.
pub fn install_jdk(java_version: &str, distribution: JdkDistribution) -> miette::Result<JdkInfo> {
    let dir_name = format!(
        "{}-{}",
        match distribution {
            JdkDistribution::Temurin => "temurin",
            JdkDistribution::Corretto => "corretto",
            JdkDistribution::Zulu => "zulu",
        },
        java_version
    );
    let dest = jdks_dir().join(&dir_name);

    if dest.is_dir() {
        if let Some(info) = validate_jdk(&dest) {
            println!("  JDK {} ({distribution}) already installed.", java_version);
            return Ok(info);
        }
    }

    let url = jdk_download_url(java_version, distribution)?;
    println!("  Installing {distribution} JDK {java_version}...");

    let tmp_dir = tempfile::tempdir().map_err(KargoError::Io)?;
    let archive_name = if cfg!(windows) {
        "jdk.zip"
    } else {
        "jdk.tar.gz"
    };
    let archive_path = tmp_dir.path().join(archive_name);

    download::download_file(&url, &archive_path)?;

    kargo_util::fs::ensure_dir(&jdks_dir()).map_err(KargoError::Io)?;

    if cfg!(windows) || url.ends_with(".zip") {
        super::extract_zip_to(&archive_path, &dest)?;
    } else {
        super::extract_tarball_to(&archive_path, &dest)?;
    }

    flatten_jdk_dir(&dest)?;

    match validate_jdk(&dest) {
        Some(info) => {
            println!(
                "  JDK {} installed at {}",
                info.version,
                info.home.display()
            );
            Ok(info)
        }
        None => Err(KargoError::Toolchain {
            message: format!(
                "JDK installation at {} does not contain a valid java binary",
                dest.display()
            ),
        }
        .into()),
    }
}

fn jdk_download_url(version: &str, dist: JdkDistribution) -> miette::Result<String> {
    let os = if cfg!(target_os = "macos") {
        "mac"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        return Err(KargoError::Toolchain {
            message: "Unsupported OS for JDK download".to_string(),
        }
        .into());
    };

    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86_64") {
        "x64"
    } else {
        return Err(KargoError::Toolchain {
            message: "Unsupported architecture for JDK download".to_string(),
        }
        .into());
    };

    let ext = if cfg!(target_os = "windows") {
        "zip"
    } else {
        "tar.gz"
    };

    match dist {
        JdkDistribution::Temurin => {
            Ok(format!(
                "https://api.adoptium.net/v3/binary/latest/{version}/ga/{os}/{arch}/jdk/hotspot/normal/eclipse?project=jdk",
            ))
        }
        JdkDistribution::Corretto => {
            let corretto_os = match os {
                "mac" => "macosx",
                _ => os,
            };
            Ok(format!(
                "https://corretto.aws/downloads/latest/amazon-corretto-{version}-{arch}-{corretto_os}-jdk.{ext}"
            ))
        }
        JdkDistribution::Zulu => {
            let zulu_os = match os {
                "mac" => "macosx",
                _ => os,
            };
            let zulu_arch = match arch {
                "aarch64" => "aarch64",
                _ => "x64",
            };
            Ok(format!(
                "https://cdn.azul.com/zulu/bin/zulu{version}.0.0-ca-jdk{version}.0.0-{zulu_os}_{zulu_arch}.{ext}"
            ))
        }
    }
}

/// List installed Kargo-managed JDKs.
pub fn list_installed_jdks() -> Vec<JdkInfo> {
    let dir = jdks_dir();
    fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(|e| {
            let path = e.ok()?.path();
            validate_jdk(&path)
        })
        .collect()
}

/// Remove a Kargo-managed JDK by major version (e.g., "21").
pub fn remove_jdk(major_version: &str) -> miette::Result<u32> {
    let dir = jdks_dir();
    let mut removed = 0u32;

    let entries: Vec<_> = fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .collect();

    for entry in entries {
        let path = entry.path();
        if let Some(info) = validate_jdk(&path) {
            if info.version == major_version {
                fs::remove_dir_all(&path).map_err(KargoError::Io)?;
                println!("  Removed JDK {} at {}", info.version, path.display());
                removed += 1;
            }
        }
    }

    if removed == 0 {
        return Err(KargoError::Toolchain {
            message: format!(
                "No managed JDK with version {major_version} found.\n  \
                 Run `kargo toolchain list` to see installed JDKs."
            ),
        }
        .into());
    }

    Ok(removed)
}

/// If a directory contains exactly one child directory, promote its contents.
fn flatten_jdk_dir(dir: &Path) -> miette::Result<()> {
    let entries: Vec<_> = fs::read_dir(dir)
        .map_err(KargoError::Io)?
        .filter_map(|e| e.ok())
        .collect();

    if entries.len() == 1 && entries[0].path().is_dir() {
        let child = entries[0].path();

        let jdk_home = if child.join("Contents/Home/bin/java").exists() {
            child.join("Contents/Home")
        } else if child.join("bin/java").exists() {
            child.clone()
        } else {
            return Ok(());
        };

        if jdk_home != *dir {
            let dir_name = dir
                .file_name()
                .unwrap_or(std::ffi::OsStr::new("jdk"))
                .to_string_lossy();
            let tmp = dir.with_file_name(format!(".kargo-jdk-flatten-{dir_name}"));
            fs::rename(dir, &tmp).map_err(KargoError::Io)?;
            fs::rename(
                jdk_home
                    .to_string_lossy()
                    .replace(&*dir.to_string_lossy(), &tmp.to_string_lossy()),
                dir,
            )
            .or_else(|_| {
                let src = tmp.join(jdk_home.strip_prefix(dir).unwrap_or(jdk_home.as_path()));
                if src.is_dir() {
                    fs::rename(&src, dir)
                } else {
                    fs::rename(&tmp, dir)
                }
            })
            .map_err(KargoError::Io)?;
            let _ = fs::remove_dir_all(&tmp);
        }
    }
    Ok(())
}
