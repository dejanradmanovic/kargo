use kargo_util::hash::{sha256_bytes, sha256_file};
use std::io::Write;
use std::path::Path;
use tempfile::NamedTempFile;

#[test]
fn test_sha256_bytes_empty() {
    let hash = sha256_bytes(b"");
    assert_eq!(
        hash,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn test_sha256_bytes_hello() {
    let hash = sha256_bytes(b"hello");
    assert_eq!(
        hash,
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
}

#[test]
fn test_sha256_bytes_deterministic() {
    let a = sha256_bytes(b"kargo");
    let b = sha256_bytes(b"kargo");
    assert_eq!(a, b);
}

#[test]
fn test_sha256_file_matches_bytes() {
    let mut tmp = NamedTempFile::new().unwrap();
    tmp.write_all(b"hello").unwrap();
    tmp.flush().unwrap();
    let file_hash = sha256_file(tmp.path()).unwrap();
    let bytes_hash = sha256_bytes(b"hello");
    assert_eq!(file_hash, bytes_hash);
}

#[test]
fn test_sha256_file_empty() {
    let tmp = NamedTempFile::new().unwrap();
    let hash = sha256_file(tmp.path()).unwrap();
    assert_eq!(hash, sha256_bytes(b""));
}

#[test]
fn test_sha256_file_not_found() {
    let result = sha256_file(Path::new("/nonexistent/path/file.txt"));
    assert!(result.is_err());
}
