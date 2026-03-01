use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

/// Compute the SHA-256 hash of a file, returning a lowercase hex string.
pub fn sha256_file(path: &Path) -> miette::Result<String> {
    use crate::errors::KargoError;

    let mut file = std::fs::File::open(path).map_err(KargoError::Io)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = file.read(&mut buffer).map_err(KargoError::Io)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Compute the SHA-256 hash of a file using streaming (BufReader), returning
/// a lowercase hex string. Does not load the entire file into memory.
pub fn sha256_file_streaming(path: &std::path::Path) -> std::io::Result<String> {
    use sha2::Digest;
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::with_capacity(64 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Compute the SHA-256 hash of a byte slice, returning a lowercase hex string.
pub fn sha256_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}
