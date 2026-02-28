use std::str::FromStr;

use kargo_toolchain::version::KotlinVersion;

#[test]
fn parse_valid_version() {
    let v = KotlinVersion::from_str("2.3.0").unwrap();
    assert_eq!(v.major(), 2);
    assert_eq!(v.minor(), 3);
    assert_eq!(v.patch(), 0);
}

#[test]
fn parse_pre_release_version() {
    let v = KotlinVersion::from_str("2.1.0").unwrap();
    assert_eq!(v.to_string(), "2.1.0");
}

#[test]
fn parse_invalid_version() {
    assert!(KotlinVersion::from_str("not-a-version").is_err());
    assert!(KotlinVersion::from_str("").is_err());
    assert!(KotlinVersion::from_str("2").is_err());
    assert!(KotlinVersion::from_str("2.3").is_err());
}

#[test]
fn version_display() {
    let v = KotlinVersion::from_str("2.3.0").unwrap();
    assert_eq!(format!("{v}"), "2.3.0");
}

#[test]
fn version_ordering() {
    let v1 = KotlinVersion::from_str("1.9.0").unwrap();
    let v2 = KotlinVersion::from_str("2.0.0").unwrap();
    let v3 = KotlinVersion::from_str("2.3.0").unwrap();

    assert!(v1 < v2);
    assert!(v2 < v3);
    assert!(v1 < v3);
}

#[test]
fn version_equality() {
    let a = KotlinVersion::from_str("2.3.0").unwrap();
    let b = KotlinVersion::from_str("2.3.0").unwrap();
    assert_eq!(a, b);
}

#[test]
fn version_new() {
    let v = KotlinVersion::new(2, 3, 0);
    assert_eq!(v.to_string(), "2.3.0");
}

#[test]
fn version_from_manifest_valid() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("Kargo.toml");
    std::fs::write(
        &manifest,
        r#"
[package]
name = "test-project"
version = "0.1.0"
kotlin = "2.3.0"
"#,
    )
    .unwrap();

    let v = KotlinVersion::from_manifest(&manifest).unwrap();
    assert_eq!(v.to_string(), "2.3.0");
}

#[test]
fn version_from_manifest_missing_field() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = dir.path().join("Kargo.toml");
    std::fs::write(
        &manifest,
        r#"
[package]
name = "test-project"
version = "0.1.0"
"#,
    )
    .unwrap();

    assert!(KotlinVersion::from_manifest(&manifest).is_err());
}
