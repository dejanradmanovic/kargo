use std::str::FromStr;

use kargo_toolchain::install;
use kargo_toolchain::version::KotlinVersion;

#[test]
fn is_installed_returns_false_for_missing() {
    let v = KotlinVersion::from_str("99.99.99").unwrap();
    assert!(!install::is_installed(&v));
}

#[test]
fn set_and_get_default() {
    // Use a unique version to avoid interfering with real state
    let v = KotlinVersion::from_str("0.0.1").unwrap();

    // Set default
    install::set_default(&v).unwrap();

    let got = install::get_default();
    assert_eq!(got, Some(v.clone()));

    // Clean up: restore previous default or remove
    let marker = kargo_util::dirs_path().join("default-kotlin");
    let _ = std::fs::remove_file(marker);
}

#[test]
fn list_installed_returns_sorted() {
    let versions = install::list_installed();
    let mut sorted = versions.clone();
    sorted.sort();
    assert_eq!(versions, sorted);
}

#[test]
fn toolchain_dir_format() {
    let v = KotlinVersion::from_str("2.3.0").unwrap();
    let dir = install::toolchain_dir(&v);
    assert!(dir.to_string_lossy().contains("kotlin-2.3.0"));
}
