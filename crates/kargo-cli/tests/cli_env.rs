use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[allow(deprecated)]
fn kargo_cmd() -> Command {
    Command::cargo_bin("kargo").unwrap()
}

#[test]
fn test_env_without_manifest_fails() {
    let tmp = TempDir::new().unwrap();

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["env"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Could not find Kargo.toml"));
}

#[test]
fn test_env_empty_project_shows_no_entries() {
    let tmp = TempDir::new().unwrap();
    let project_name = "env-test";

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["new", project_name])
        .assert()
        .success();

    kargo_cmd()
        .current_dir(tmp.path().join(project_name))
        .args(["env"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "No environment variables configured.",
        ));
}

#[test]
fn test_env_shows_entries_masked() {
    let tmp = TempDir::new().unwrap();
    let project_name = "env-masked";

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["new", project_name])
        .assert()
        .success();

    let project_dir = tmp.path().join(project_name);
    fs::write(
        project_dir.join(".kargo.env"),
        "MAVEN_TOKEN=abc123\nNEXUS_PASSWORD=s3cret\n",
    )
    .unwrap();

    kargo_cmd()
        .current_dir(&project_dir)
        .args(["env"])
        .assert()
        .success()
        .stdout(predicate::str::contains(".kargo.env (2 entries)"))
        .stdout(predicate::str::contains("MAVEN_TOKEN = ********"))
        .stdout(predicate::str::contains("NEXUS_PASSWORD = ********"))
        .stdout(predicate::str::contains("abc123").not())
        .stdout(predicate::str::contains("s3cret").not());
}

#[test]
fn test_env_reveal_shows_values() {
    let tmp = TempDir::new().unwrap();
    let project_name = "env-reveal";

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["new", project_name])
        .assert()
        .success();

    let project_dir = tmp.path().join(project_name);
    fs::write(project_dir.join(".kargo.env"), "MAVEN_TOKEN=abc123\n").unwrap();

    kargo_cmd()
        .current_dir(&project_dir)
        .args(["env", "--reveal"])
        .assert()
        .success()
        .stdout(predicate::str::contains("MAVEN_TOKEN = abc123"));
}
