use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn kargo_cmd() -> Command {
    Command::cargo_bin("kargo").unwrap()
}

#[test]
fn test_clean_removes_build_directory() {
    let tmp = TempDir::new().unwrap();

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["new", "clean-test"])
        .assert()
        .success();

    let project_dir = tmp.path().join("clean-test");
    let build_dir = project_dir.join("build");
    fs::create_dir_all(&build_dir).unwrap();
    fs::write(build_dir.join("output.jar"), "fake").unwrap();
    assert!(build_dir.exists());

    kargo_cmd()
        .current_dir(&project_dir)
        .args(["clean"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Cleaned build directory"));

    assert!(!build_dir.exists());
}

#[test]
fn test_clean_no_build_dir_prints_nothing_to_clean() {
    let tmp = TempDir::new().unwrap();

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["new", "no-build-test"])
        .assert()
        .success();

    let project_dir = tmp.path().join("no-build-test");

    kargo_cmd()
        .current_dir(&project_dir)
        .args(["clean"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Nothing to clean"));
}

#[test]
fn test_clean_specific_variant() {
    let tmp = TempDir::new().unwrap();

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["new", "variant-clean-test"])
        .assert()
        .success();

    let project_dir = tmp.path().join("variant-clean-test");
    let variant_dir = project_dir.join("build/variants/freeStagingDev");
    fs::create_dir_all(&variant_dir).unwrap();
    fs::write(variant_dir.join("app.jar"), "fake").unwrap();

    kargo_cmd()
        .current_dir(&project_dir)
        .args(["clean", "--variant", "freeStagingDev"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Cleaned variant"));

    assert!(!variant_dir.exists());
    assert!(project_dir.join("build/variants").exists());
}

#[test]
fn test_clean_nonexistent_variant() {
    let tmp = TempDir::new().unwrap();

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["new", "no-variant-test"])
        .assert()
        .success();

    kargo_cmd()
        .current_dir(tmp.path().join("no-variant-test"))
        .args(["clean", "--variant", "noSuchVariant"])
        .assert()
        .success()
        .stdout(predicate::str::contains("does not exist"));
}

#[test]
fn test_clean_without_manifest_fails() {
    let tmp = TempDir::new().unwrap();

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["clean"])
        .assert()
        .failure();
}
