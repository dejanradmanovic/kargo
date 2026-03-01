//! SDK detection and installation: JDK, Android SDK, Xcode.

mod android;
mod jdk;
mod xcode;

pub use android::*;
pub use jdk::*;
pub use xcode::*;

// -----------------------------------------------------------------------
// Shared archive extraction helpers used by jdk and android submodules.
// -----------------------------------------------------------------------

use std::fs;
use std::path::Path;
use std::process::Command;

use kargo_util::errors::KargoError;

fn extract_zip_to(zip_path: &Path, dest: &Path) -> miette::Result<()> {
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
            std::io::copy(&mut entry, &mut out).map_err(|e| KargoError::Toolchain {
                message: format!("Extract error: {e}"),
            })?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = entry.unix_mode() {
                    if let Err(e) = fs::set_permissions(&out_path, fs::Permissions::from_mode(mode))
                    {
                        tracing::warn!("Failed to set permissions on {}: {e}", out_path.display());
                    }
                }
            }
        }
    }
    Ok(())
}

fn extract_tarball_to(tar_gz_path: &Path, dest: &Path) -> miette::Result<()> {
    fs::create_dir_all(dest).map_err(KargoError::Io)?;

    let status = Command::new("tar")
        .args(["xzf", &tar_gz_path.to_string_lossy(), "-C"])
        .arg(dest)
        .status()
        .map_err(|e| KargoError::Toolchain {
            message: format!("Failed to run tar: {e}"),
        })?;

    if !status.success() {
        return Err(KargoError::Toolchain {
            message: format!("tar extraction failed for {}", tar_gz_path.display()),
        }
        .into());
    }
    Ok(())
}
