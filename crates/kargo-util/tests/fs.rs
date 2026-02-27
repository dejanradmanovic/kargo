use kargo_util::fs::{ensure_dir, find_ancestor_with};
use tempfile::TempDir;

#[test]
fn test_find_ancestor_with_direct() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("Kargo.toml"), "").unwrap();
    let result = find_ancestor_with(tmp.path(), "Kargo.toml");
    assert_eq!(result, Some(tmp.path().to_path_buf()));
}

#[test]
fn test_find_ancestor_with_nested() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("Kargo.toml"), "").unwrap();
    let nested = tmp.path().join("a").join("b").join("c");
    std::fs::create_dir_all(&nested).unwrap();
    let result = find_ancestor_with(&nested, "Kargo.toml");
    assert_eq!(result, Some(tmp.path().to_path_buf()));
}

#[test]
fn test_find_ancestor_with_not_found() {
    let tmp = TempDir::new().unwrap();
    let result = find_ancestor_with(tmp.path(), "NonExistent.file");
    assert_eq!(result, None);
}

#[test]
fn test_ensure_dir_creates_nested() {
    let tmp = TempDir::new().unwrap();
    let deep = tmp.path().join("x").join("y").join("z");
    assert!(!deep.exists());
    ensure_dir(&deep).unwrap();
    assert!(deep.is_dir());
}

#[test]
fn test_ensure_dir_idempotent() {
    let tmp = TempDir::new().unwrap();
    let dir = tmp.path().join("already");
    std::fs::create_dir(&dir).unwrap();
    ensure_dir(&dir).unwrap();
    assert!(dir.is_dir());
}
