use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

#[allow(deprecated)]
fn kargo_cmd() -> Command {
    Command::cargo_bin("kargo").unwrap()
}

#[test]
fn test_new_jvm_project() {
    let tmp = TempDir::new().unwrap();
    let project_name = "test-jvm-app";

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["new", project_name])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created new Kargo project"));

    let project_dir = tmp.path().join(project_name);
    assert!(project_dir.join("Kargo.toml").is_file());
    assert!(project_dir.join("Kargo.lock").is_file());
    assert!(project_dir.join(".gitignore").is_file());
    assert!(project_dir.join(".kargo.env").is_file());
    assert!(!project_dir.join("local.properties").exists());
    assert!(project_dir.join("src/main/kotlin").is_dir());
    assert!(project_dir.join("src/test/kotlin").is_dir());
    assert!(project_dir
        .join("src/main/kotlin/com/example/Main.kt")
        .is_file());
    assert!(project_dir
        .join("src/test/kotlin/com/example/MainTest.kt")
        .is_file());

    let manifest = fs::read_to_string(project_dir.join("Kargo.toml")).unwrap();
    assert!(manifest.contains(&format!("name = \"{project_name}\"")));
    assert!(manifest.contains("group = \"com.example\""));
    assert!(manifest.contains("kotlin = \"2.3.0\""));
    assert!(manifest.contains("[targets.jvm]"));

    let main_kt =
        fs::read_to_string(project_dir.join("src/main/kotlin/com/example/Main.kt")).unwrap();
    assert!(main_kt.contains("fun main()"));
    assert!(main_kt.contains(project_name));
}

#[test]
fn test_new_kmp_project() {
    let tmp = TempDir::new().unwrap();
    let project_name = "test-kmp-app";

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["new", project_name, "--template", "kmp"])
        .assert()
        .success();

    let project_dir = tmp.path().join(project_name);
    assert!(project_dir.join("src/commonMain/kotlin").is_dir());
    assert!(project_dir.join("src/commonMain/resources").is_dir());
    assert!(project_dir.join("src/commonTest/kotlin").is_dir());
    assert!(project_dir.join("src/jvmMain/kotlin").is_dir());
    assert!(project_dir.join("src/jvmTest/kotlin").is_dir());
    assert!(project_dir.join("src/iosMain/kotlin").is_dir());
    assert!(project_dir.join("src/iosTest/kotlin").is_dir());
    assert!(project_dir
        .join("src/commonMain/kotlin/Greeting.kt")
        .is_file());

    let manifest = fs::read_to_string(project_dir.join("Kargo.toml")).unwrap();
    assert!(manifest.contains("ios-arm64"));
    assert!(manifest.contains("ios-simulator-arm64"));
    assert!(
        !manifest.contains("[compose]"),
        "kmp should not enable compose"
    );
    assert!(
        !manifest.contains("android"),
        "kmp should not include android target"
    );
}

#[test]
fn test_new_cmp_project() {
    let tmp = TempDir::new().unwrap();
    let project_name = "test-cmp-app";

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["new", project_name, "--template", "cmp"])
        .assert()
        .success();

    let project_dir = tmp.path().join(project_name);

    assert!(project_dir.join("src/commonMain/kotlin").is_dir());
    assert!(project_dir.join("src/androidMain/kotlin").is_dir());
    assert!(project_dir.join("src/desktopMain/kotlin").is_dir());
    assert!(project_dir.join("src/iosMain/kotlin").is_dir());
    assert!(project_dir.join("src/jvmMain/kotlin").is_dir());

    let manifest = fs::read_to_string(project_dir.join("Kargo.toml")).unwrap();
    assert!(manifest.contains("[compose]"));
    assert!(manifest.contains("enabled = true"));
    assert!(manifest.contains("android"));
    assert!(manifest.contains("min-sdk"));
    assert!(manifest.contains("ios-arm64"));

    let app_kt = fs::read_to_string(project_dir.join("src/commonMain/kotlin/App.kt")).unwrap();
    assert!(app_kt.contains("@Composable"));
    assert!(app_kt.contains(project_name));
}

#[test]
fn test_new_android_project() {
    let tmp = TempDir::new().unwrap();
    let project_name = "test-android-app";

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["new", project_name, "--template", "android"])
        .assert()
        .success();

    let project_dir = tmp.path().join(project_name);
    assert!(project_dir.join("src/main/kotlin").is_dir());
    assert!(project_dir.join("src/main/res").is_dir());
    assert!(project_dir.join("src/test/kotlin").is_dir());
    assert!(project_dir.join("src/main/AndroidManifest.xml").is_file());
    assert!(project_dir
        .join("src/main/kotlin/MainActivity.kt")
        .is_file());

    let manifest = fs::read_to_string(project_dir.join("Kargo.toml")).unwrap();
    assert!(manifest.contains("[targets.android]"));
    assert!(manifest.contains("min-sdk"));
    assert!(manifest.contains("target-sdk"));
    assert!(manifest.contains("compile-sdk"));
    assert!(
        !manifest.contains("[targets.jvm]"),
        "android-only should not have jvm target"
    );
}

#[test]
fn test_new_lib_project() {
    let tmp = TempDir::new().unwrap();
    let project_name = "test-lib";

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["new", project_name, "--template", "lib"])
        .assert()
        .success();

    let project_dir = tmp.path().join(project_name);
    assert!(project_dir.join("src/main/kotlin/Lib.kt").is_file());
    assert!(!project_dir.join("src/main/kotlin/Main.kt").exists());

    let lib_kt = fs::read_to_string(project_dir.join("src/main/kotlin/Lib.kt")).unwrap();
    assert!(lib_kt.contains("fun greeting()"));
    assert!(!lib_kt.contains("fun main()"));

    let manifest = fs::read_to_string(project_dir.join("Kargo.toml")).unwrap();
    assert!(manifest.contains("[targets.jvm]"));
}

#[test]
fn test_new_existing_directory_fails() {
    let tmp = TempDir::new().unwrap();
    let project_name = "already-exists";
    fs::create_dir(tmp.path().join(project_name)).unwrap();

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["new", project_name])
        .assert()
        .failure();
}

#[test]
fn test_new_unknown_template_fails() {
    let tmp = TempDir::new().unwrap();

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["new", "bad-tmpl", "--template", "nonexistent"])
        .assert()
        .failure();
}

#[test]
fn test_new_gitignore_contains_required_entries() {
    let tmp = TempDir::new().unwrap();
    let project_name = "gitignore-test";

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["new", project_name])
        .assert()
        .success();

    let gitignore = fs::read_to_string(tmp.path().join(project_name).join(".gitignore")).unwrap();
    assert!(gitignore.contains("build/"));
    assert!(gitignore.contains(".kargo.env"));
    assert!(!gitignore.contains("local.properties"));
}

#[test]
fn test_new_manifest_is_parseable() {
    let tmp = TempDir::new().unwrap();
    let project_name = "parseable-test";

    kargo_cmd()
        .current_dir(tmp.path())
        .args(["new", project_name])
        .assert()
        .success();

    let manifest_content =
        fs::read_to_string(tmp.path().join(project_name).join("Kargo.toml")).unwrap();
    let manifest = kargo_core::manifest::Manifest::parse_toml(&manifest_content);
    assert!(manifest.is_ok(), "Generated Kargo.toml should be parseable");
}

#[test]
fn test_init_creates_only_core_files() {
    let tmp = TempDir::new().unwrap();
    let project_dir = tmp.path().join("existing-project");
    fs::create_dir(&project_dir).unwrap();
    fs::create_dir_all(project_dir.join("src/main/kotlin")).unwrap();
    fs::write(project_dir.join("src/main/kotlin/App.kt"), "fun main() {}").unwrap();

    kargo_cmd()
        .current_dir(&project_dir)
        .args(["init", "--template", "kmp"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized Kargo project"));

    assert!(project_dir.join("Kargo.toml").is_file());
    assert!(project_dir.join("Kargo.lock").is_file());
    assert!(project_dir.join(".gitignore").is_file());
    assert!(project_dir.join(".kargo.env").is_file());

    assert!(
        !project_dir.join("src/commonMain").exists(),
        "init must not create source directories"
    );
    assert!(
        project_dir.join("src/main/kotlin/App.kt").is_file(),
        "init must not touch existing source files"
    );

    let manifest = fs::read_to_string(project_dir.join("Kargo.toml")).unwrap();
    assert!(
        manifest.contains("ios-arm64"),
        "manifest should use kmp template"
    );
}

#[test]
fn test_init_does_not_overwrite_existing_files() {
    let tmp = TempDir::new().unwrap();
    let project_dir = tmp.path().join("has-gitignore");
    fs::create_dir(&project_dir).unwrap();
    fs::write(project_dir.join(".gitignore"), "my-custom-ignores\n").unwrap();

    kargo_cmd()
        .current_dir(&project_dir)
        .args(["init"])
        .assert()
        .success();

    let gitignore = fs::read_to_string(project_dir.join(".gitignore")).unwrap();
    assert_eq!(
        gitignore, "my-custom-ignores\n",
        "init must not overwrite existing .gitignore"
    );
    assert!(project_dir.join("Kargo.toml").is_file());
}

#[test]
fn test_all_templates_produce_parseable_manifests() {
    for template in &["jvm", "kmp", "cmp", "android", "lib"] {
        let tmp = TempDir::new().unwrap();
        let project_name = format!("parse-{}", template);

        kargo_cmd()
            .current_dir(tmp.path())
            .args(["new", &project_name, "--template", template])
            .assert()
            .success();

        let manifest_content =
            fs::read_to_string(tmp.path().join(&project_name).join("Kargo.toml")).unwrap();
        let result = kargo_core::manifest::Manifest::parse_toml(&manifest_content);
        assert!(
            result.is_ok(),
            "Template '{}' generated unparseable Kargo.toml: {:?}",
            template,
            result.err()
        );
    }
}
