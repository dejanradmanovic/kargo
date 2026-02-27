use kargo_core::manifest::Manifest;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures")
}

#[test]
fn test_parse_simple_jvm_fixture() {
    let path = fixtures_dir().join("simple-jvm.toml");
    let manifest = Manifest::from_path(&path).unwrap();
    assert_eq!(manifest.package.name, "my-jvm-app");
    assert_eq!(manifest.package.version, "0.1.0");
    assert_eq!(manifest.package.kotlin, "2.3.0");
    assert_eq!(manifest.package.description.as_deref(), Some("A simple JVM application"));
    assert_eq!(manifest.package.license.as_deref(), Some("MIT"));
    assert_eq!(manifest.targets.len(), 1);
    assert!(manifest.targets.contains_key("jvm"));
    assert_eq!(
        manifest.targets["jvm"].java_target.as_deref(),
        Some("21")
    );
    assert_eq!(manifest.dependencies.len(), 1);
    assert_eq!(manifest.dev_dependencies.len(), 1);
    assert_eq!(manifest.profile.len(), 2);
}

#[test]
fn test_parse_kmp_compose_fixture() {
    let path = fixtures_dir().join("kmp-compose.toml");
    let manifest = Manifest::from_path(&path).unwrap();
    assert_eq!(manifest.package.name, "my-kmp-compose");
    assert_eq!(manifest.targets.len(), 5);
    assert!(manifest.targets.contains_key("jvm"));
    assert!(manifest.targets.contains_key("ios-arm64"));
    assert!(manifest.targets.contains_key("ios-simulator-arm64"));
    assert!(manifest.targets.contains_key("js"));
    assert!(manifest.targets.contains_key("wasm-js"));
    assert!(manifest.compose.as_ref().unwrap().enabled);
    assert_eq!(manifest.dependencies.len(), 2);
    assert_eq!(manifest.plugins.len(), 1);
    assert_eq!(manifest.hooks.len(), 2);
    assert_eq!(manifest.lint.as_ref().unwrap().rules.len(), 3);
    assert_eq!(manifest.format.as_ref().unwrap().max_line_length, Some(120));
    let ws = manifest.workspace.as_ref().unwrap();
    assert_eq!(ws.members, vec!["app", "shared"]);
}

#[test]
fn test_parse_with_flavors_fixture() {
    let path = fixtures_dir().join("with-flavors.toml");
    let manifest = Manifest::from_path(&path).unwrap();
    assert_eq!(manifest.package.name, "flavored-app");
    let flavors = manifest.flavors.as_ref().unwrap();
    assert_eq!(flavors.dimensions, vec!["tier", "environment"]);
    let default = flavors.default.as_ref().unwrap();
    assert_eq!(default.get("tier").unwrap(), "paid");
    assert_eq!(default.get("environment").unwrap(), "staging");
    assert_eq!(manifest.flavor.len(), 2);
}

#[test]
fn test_parse_with_catalog_fixture() {
    let path = fixtures_dir().join("with-catalog.toml");
    let manifest = Manifest::from_path(&path).unwrap();
    assert_eq!(manifest.package.name, "catalog-project");
    let catalog = manifest.catalog.as_ref().unwrap();
    assert_eq!(catalog.versions.len(), 3);
    assert_eq!(catalog.libraries.len(), 2);
    assert_eq!(catalog.bundles.len(), 1);
    assert_eq!(
        catalog.bundles.get("ktor-server").unwrap(),
        &vec!["ktor-server-core".to_string()]
    );
}

#[test]
fn test_parse_invalid_missing_name_fixture() {
    let path = fixtures_dir().join("invalid-missing-name.toml");
    let result = Manifest::from_path(&path);
    assert!(result.is_err(), "Manifest without name should fail to parse");
}

#[test]
fn test_parse_nonexistent_fixture() {
    let path = fixtures_dir().join("does-not-exist.toml");
    let result = Manifest::from_path(&path);
    assert!(result.is_err());
}
