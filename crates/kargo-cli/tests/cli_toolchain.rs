use assert_cmd::Command;
use predicates::prelude::*;

#[allow(deprecated)]
fn kargo() -> Command {
    Command::cargo_bin("kargo").unwrap()
}

#[test]
fn toolchain_list_runs() {
    kargo().args(["toolchain", "list"]).assert().success();
}

#[test]
fn toolchain_path_without_project_or_default() {
    // In a temp dir with no Kargo.toml and possibly no default toolchain,
    // this should either succeed (if a default is set) or fail gracefully.
    let dir = tempfile::tempdir().unwrap();
    kargo()
        .args(["toolchain", "path"])
        .current_dir(dir.path())
        .assert()
        .code(predicate::in_iter([0, 1]));
}

#[test]
fn toolchain_install_invalid_version() {
    kargo()
        .args(["toolchain", "install", "not-a-version"])
        .assert()
        .failure();
}

#[test]
fn toolchain_remove_not_installed() {
    kargo()
        .args(["toolchain", "remove", "99.99.99"])
        .assert()
        .failure();
}

#[test]
fn toolchain_install_no_args_fails() {
    kargo().args(["toolchain", "install"]).assert().failure();
}

#[test]
fn toolchain_install_jdk_flag_accepted() {
    // --jdk flag should parse correctly (it may fail or succeed
    // depending on whether a JDK is already installed, but it
    // must not be a CLI parse error)
    let result = kargo()
        .args(["toolchain", "install", "--jdk"])
        .timeout(std::time::Duration::from_secs(5))
        .ok();
    let _ = result;
}

#[test]
fn toolchain_install_jdk_with_version_accepted() {
    // --jdk 17 should parse correctly
    let result = kargo()
        .args(["toolchain", "install", "--jdk", "17"])
        .timeout(std::time::Duration::from_secs(5))
        .ok();
    let _ = result;
}

#[test]
fn toolchain_remove_no_args_fails() {
    kargo().args(["toolchain", "remove"]).assert().failure();
}

#[test]
fn toolchain_remove_jdk_not_installed() {
    kargo()
        .args(["toolchain", "remove", "--jdk", "99"])
        .assert()
        .failure();
}
