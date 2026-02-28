use kargo_core::manifest::Manifest;

const MINIMAL_TOML: &str = r#"
[package]
name = "test-app"
version = "0.1.0"
kotlin = "2.3.0"
"#;

const KMP_TOML: &str = r#"
[package]
name = "kmp-app"
version = "1.0.0"
kotlin = "2.3.0"
description = "A KMP app"
authors = ["Jane Doe <jane@example.com>"]

[targets]
jvm = { java-target = "21" }
ios-arm64 = {}
js = { module-kind = "es" }

[compose]
enabled = true

[dependencies]
kotlinx-coroutines = "org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0"

[dev-dependencies]
kotlin-test = "org.jetbrains.kotlin:kotlin-test:2.3.0"

[hooks]
pre-build = ["fmt --check", "lint"]

[lint]
rules = ["naming", "style"]
severity = "warning"

[format]
style = "official"
indent = 4
max-line-length = 120

[profile.dev]
debug = true

[profile.release]
optimization = true
"#;

#[test]
fn test_parse_minimal_manifest() {
    let manifest = Manifest::parse_toml(MINIMAL_TOML).unwrap();
    assert_eq!(manifest.package.name, "test-app");
    assert_eq!(manifest.package.version, "0.1.0");
    assert_eq!(manifest.package.kotlin, "2.3.0");
    assert!(manifest.targets.is_empty());
    assert!(manifest.dependencies.is_empty());
}

#[test]
fn test_parse_kmp_manifest() {
    let manifest = Manifest::parse_toml(KMP_TOML).unwrap();
    assert_eq!(manifest.package.name, "kmp-app");
    assert_eq!(manifest.package.description.as_deref(), Some("A KMP app"));
    assert_eq!(manifest.package.authors.len(), 1);
    assert_eq!(manifest.targets.len(), 3);
    assert!(manifest.targets.contains_key("jvm"));
    assert!(manifest.targets.contains_key("ios-arm64"));
    assert!(manifest.targets.contains_key("js"));
    assert!(manifest.compose.as_ref().unwrap().enabled);
    assert_eq!(manifest.dependencies.len(), 1);
    assert_eq!(manifest.dev_dependencies.len(), 1);
    assert_eq!(manifest.hooks.get("pre-build").unwrap().len(), 2);
    assert_eq!(manifest.lint.as_ref().unwrap().rules.len(), 2);
    assert_eq!(manifest.format.as_ref().unwrap().indent, Some(4));
    assert_eq!(manifest.profile.len(), 2);
}

#[test]
fn test_parse_manifest_with_repositories() {
    let toml = r#"
[package]
name = "test"
version = "0.1.0"
kotlin = "2.3.0"

[repositories]
central = "https://repo.maven.apache.org/maven2"
"#;
    let manifest = Manifest::parse_toml(toml).unwrap();
    assert_eq!(manifest.repositories.len(), 1);
}

#[test]
fn test_parse_manifest_missing_package_fails() {
    let toml = r#"
[dependencies]
foo = "bar"
"#;
    let result = Manifest::parse_toml(toml);
    assert!(result.is_err());
}

#[test]
fn test_parse_manifest_missing_name_fails() {
    let toml = r#"
[package]
version = "0.1.0"
kotlin = "2.3.0"
"#;
    let result = Manifest::parse_toml(toml);
    assert!(result.is_err());
}

#[test]
fn test_parse_manifest_empty_string_fails() {
    let result = Manifest::parse_toml("");
    assert!(result.is_err());
}

#[test]
fn test_parse_manifest_with_toolchain() {
    let toml = r#"
[package]
name = "test"
version = "0.1.0"
kotlin = "2.3.0"

[toolchain]
jdk = "21"
auto-download = false
"#;
    let manifest = Manifest::parse_toml(toml).unwrap();
    let tc = manifest.toolchain.unwrap();
    assert_eq!(tc.jdk.as_deref(), Some("21"));
    assert_eq!(tc.auto_download, Some(false));
}

#[test]
fn test_parse_manifest_with_workspace() {
    let toml = r#"
[package]
name = "root"
version = "0.1.0"
kotlin = "2.3.0"

[workspace]
members = ["app", "shared", "libs/*"]
"#;
    let manifest = Manifest::parse_toml(toml).unwrap();
    let ws = manifest.workspace.unwrap();
    assert_eq!(ws.members.len(), 3);
}

#[test]
fn test_parse_manifest_from_nonexistent_path() {
    let result = Manifest::from_path(std::path::Path::new("/nonexistent/Kargo.toml"));
    assert!(result.is_err());
}

#[test]
fn test_parse_android_target_inline_style() {
    let toml = r#"
[package]
name = "test"
version = "0.1.0"
kotlin = "2.3.0"

[targets]
jvm = { java-target = "21" }
android = { min-sdk = 24, target-sdk = 35, compile-sdk = 35 }
ios-arm64 = {}
ios-simulator-arm64 = {}
"#;
    let manifest = Manifest::parse_toml(toml).unwrap();
    assert_eq!(manifest.targets.len(), 4);
    let android = manifest.targets.get("android").unwrap();
    assert_eq!(android.min_sdk, Some(24));
    assert_eq!(android.target_sdk, Some(35));
    assert_eq!(android.compile_sdk, Some(35));
}

#[test]
fn test_parse_android_target_section_style() {
    let toml = r#"
[package]
name = "test"
version = "0.1.0"
kotlin = "2.3.0"

[targets]
jvm = { java-target = "21" }
ios-arm64 = {}
ios-simulator-arm64 = {}

[targets.android]
min-sdk = 24
target-sdk = 35
compile-sdk = 35
"#;
    let manifest = Manifest::parse_toml(toml).unwrap();
    assert_eq!(manifest.targets.len(), 4);
    let android = manifest.targets.get("android").unwrap();
    assert_eq!(android.min_sdk, Some(24));
    assert_eq!(android.target_sdk, Some(35));
    assert_eq!(android.compile_sdk, Some(35));
}
