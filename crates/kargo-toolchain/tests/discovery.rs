use std::str::FromStr;

use kargo_toolchain::discovery;
use kargo_toolchain::version::KotlinVersion;

#[tokio::test]
async fn resolve_toolchain_fails_for_missing_version_without_auto_download() {
    let v = KotlinVersion::from_str("99.99.99").unwrap();
    let result = discovery::resolve_toolchain(&v, false, None).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn resolve_project_toolchain_fails_without_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let result = discovery::resolve_project_toolchain(dir.path(), false, None).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn resolve_project_toolchain_reads_manifest() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Kargo.toml"),
        r#"
[package]
name = "test"
version = "0.1.0"
kotlin = "99.99.99"
"#,
    )
    .unwrap();

    let result = discovery::resolve_project_toolchain(dir.path(), false, None).await;
    assert!(result.is_err());
}
