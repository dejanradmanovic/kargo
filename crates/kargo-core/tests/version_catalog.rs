use kargo_core::manifest::{CatalogConfig, CatalogLibrary};
use kargo_core::version_catalog::resolve_catalog;
use std::collections::BTreeMap;

#[test]
fn resolve_catalog_with_version_ref() {
    let mut versions = BTreeMap::new();
    versions.insert("kotlin".to_string(), "1.9.0".to_string());
    let mut libraries = BTreeMap::new();
    libraries.insert(
        "kotlin-stdlib".to_string(),
        CatalogLibrary {
            group: "org.jetbrains.kotlin".to_string(),
            artifact: "kotlin-stdlib".to_string(),
            version_ref: Some("kotlin".to_string()),
            version: None,
        },
    );
    let catalog = CatalogConfig {
        versions,
        libraries,
        bundles: BTreeMap::new(),
        plugins: BTreeMap::new(),
    };

    let entries = resolve_catalog(&catalog);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].1.group, "org.jetbrains.kotlin");
    assert_eq!(entries[0].1.artifact, "kotlin-stdlib");
    assert_eq!(entries[0].1.version, "1.9.0");
}

#[test]
fn resolve_catalog_with_direct_version() {
    let mut libraries = BTreeMap::new();
    libraries.insert(
        "lib".to_string(),
        CatalogLibrary {
            group: "com.example".to_string(),
            artifact: "lib".to_string(),
            version_ref: None,
            version: Some("2.0.0".to_string()),
        },
    );
    let catalog = CatalogConfig {
        versions: BTreeMap::new(),
        libraries,
        bundles: BTreeMap::new(),
        plugins: BTreeMap::new(),
    };

    let entries = resolve_catalog(&catalog);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].1.version, "2.0.0");
}

#[test]
fn resolve_catalog_missing_version_ref_returns_empty_string() {
    let mut libraries = BTreeMap::new();
    libraries.insert(
        "lib".to_string(),
        CatalogLibrary {
            group: "com.example".to_string(),
            artifact: "lib".to_string(),
            version_ref: Some("nonexistent".to_string()),
            version: None,
        },
    );
    let catalog = CatalogConfig {
        versions: BTreeMap::new(),
        libraries,
        bundles: BTreeMap::new(),
        plugins: BTreeMap::new(),
    };

    let entries = resolve_catalog(&catalog);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].1.version, "");
}

#[test]
fn resolve_catalog_empty_returns_empty_vec() {
    let catalog = CatalogConfig {
        versions: BTreeMap::new(),
        libraries: BTreeMap::new(),
        bundles: BTreeMap::new(),
        plugins: BTreeMap::new(),
    };
    let entries = resolve_catalog(&catalog);
    assert!(entries.is_empty());
}
