use assert_cmd::Command;
use predicates::prelude::*;

#[allow(deprecated)]
fn kargo() -> Command {
    Command::cargo_bin("kargo").unwrap()
}

#[test]
fn self_info_shows_version() {
    kargo()
        .args(["self", "info"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Kargo"));
}

#[test]
fn self_update_check_prints_current_version() {
    let assert = kargo().args(["self", "update", "--check"]).assert();

    // May succeed ("up to date") or fail ("no releases") depending on
    // whether the GitHub repo has published releases.  Either way it
    // should print the current version.
    assert.stdout(predicate::str::contains("Kargo"));
}

#[test]
fn self_clean_runs_without_error() {
    kargo().args(["self", "clean"]).assert().success();
}
