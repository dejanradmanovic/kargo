use std::fs;
use tempfile::TempDir;

use kargo_core::manifest::Manifest;

#[test]
fn test_from_path_resolves_env_references() {
    let tmp = TempDir::new().unwrap();

    fs::write(
        tmp.path().join("Kargo.toml"),
        r#"
[package]
name = "env-test"
version = "1.0.0"
kotlin = "2.3.0"
description = "${env:MY_APP_DESC}"
"#,
    )
    .unwrap();

    fs::write(
        tmp.path().join(".kargo.env"),
        "MY_APP_DESC=A great Kotlin app\n",
    )
    .unwrap();

    let manifest = Manifest::from_path(&tmp.path().join("Kargo.toml")).unwrap();
    assert_eq!(
        manifest.package.description.as_deref(),
        Some("A great Kotlin app")
    );
}

#[test]
fn test_from_path_without_env_file_still_works() {
    let tmp = TempDir::new().unwrap();

    fs::write(
        tmp.path().join("Kargo.toml"),
        r#"
[package]
name = "no-env"
version = "1.0.0"
kotlin = "2.3.0"
"#,
    )
    .unwrap();

    let manifest = Manifest::from_path(&tmp.path().join("Kargo.toml"));
    assert!(manifest.is_ok());
}

#[test]
fn test_from_path_unresolved_env_refs_become_empty() {
    let tmp = TempDir::new().unwrap();

    fs::write(
        tmp.path().join("Kargo.toml"),
        r#"
[package]
name = "unresolved-test"
version = "1.0.0"
kotlin = "2.3.0"
description = "${env:NONEXISTENT_VAR_12345}"
"#,
    )
    .unwrap();

    let manifest = Manifest::from_path(&tmp.path().join("Kargo.toml")).unwrap();
    assert_eq!(manifest.package.description.as_deref(), Some(""));
}

#[test]
fn test_from_path_env_used_in_repository_config() {
    let tmp = TempDir::new().unwrap();

    fs::write(
        tmp.path().join("Kargo.toml"),
        r#"
[package]
name = "repo-test"
version = "1.0.0"
kotlin = "2.3.0"

[repositories]
nexus = { url = "https://nexus.example.com", username = "${env:NEXUS_USER}", password = "${env:NEXUS_PASS}" }
"#,
    )
    .unwrap();

    fs::write(
        tmp.path().join(".kargo.env"),
        "NEXUS_USER=deploy\nNEXUS_PASS=s3cret\n",
    )
    .unwrap();

    let manifest = Manifest::from_path(&tmp.path().join("Kargo.toml")).unwrap();
    let nexus = manifest.repositories.get("nexus");
    assert!(nexus.is_some());
}
