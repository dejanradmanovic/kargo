//! Xcode / Apple SDK detection.

use std::path::PathBuf;
use std::process::Command;

/// Information about a discovered Xcode installation.
#[derive(Debug, Clone)]
pub struct XcodeInfo {
    pub sdk_path: PathBuf,
    pub version: Option<String>,
}

/// Discover Xcode on macOS.
pub fn discover_xcode() -> Option<XcodeInfo> {
    #[cfg(not(target_os = "macos"))]
    {
        None
    }

    #[cfg(target_os = "macos")]
    {
        let sdk_output = Command::new("xcrun")
            .args(["--show-sdk-path"])
            .output()
            .ok()?;

        if !sdk_output.status.success() {
            return None;
        }

        let sdk_path = PathBuf::from(
            String::from_utf8_lossy(&sdk_output.stdout).trim().to_string(),
        );

        let version = Command::new("xcodebuild")
            .arg("-version")
            .output()
            .ok()
            .and_then(|out| {
                let s = String::from_utf8_lossy(&out.stdout).to_string();
                s.lines()
                    .next()
                    .and_then(|line| line.strip_prefix("Xcode "))
                    .map(|v| v.trim().to_string())
            });

        Some(XcodeInfo { sdk_path, version })
    }
}

/// Print instructions for installing Xcode.
pub fn print_xcode_instructions() {
    println!();
    println!("  Xcode is required for iOS/macOS targets.");
    println!("  Install options:");
    println!("    - Full Xcode: Download from the Mac App Store");
    println!("    - Command-line tools only: xcode-select --install");
    println!("    - Downloads: https://developer.apple.com/download/all/");
    println!();
}
