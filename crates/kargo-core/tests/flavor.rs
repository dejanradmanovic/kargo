use kargo_core::flavor::BuildVariant;
use std::collections::BTreeMap;

#[test]
fn build_variant_name_single_dimension() {
    let mut flavors = BTreeMap::new();
    flavors.insert("tier".to_string(), "free".to_string());
    let variant = BuildVariant {
        flavors,
        profile: "release".to_string(),
    };
    assert_eq!(variant.name(), "free-release");
}

#[test]
fn build_variant_name_two_dimensions() {
    let mut flavors = BTreeMap::new();
    flavors.insert("a".to_string(), "free".to_string());
    flavors.insert("b".to_string(), "staging".to_string());
    let variant = BuildVariant {
        flavors,
        profile: "release".to_string(),
    };
    assert_eq!(variant.name(), "free-staging-release");
}

#[test]
fn build_variant_camel_case_name_single_dimension() {
    let mut flavors = BTreeMap::new();
    flavors.insert("tier".to_string(), "free".to_string());
    let variant = BuildVariant {
        flavors,
        profile: "release".to_string(),
    };
    assert_eq!(variant.camel_case_name(), "freeRelease");
}

#[test]
fn build_variant_camel_case_name_two_dimensions() {
    let mut flavors = BTreeMap::new();
    flavors.insert("a".to_string(), "free".to_string());
    flavors.insert("b".to_string(), "staging".to_string());
    let variant = BuildVariant {
        flavors,
        profile: "release".to_string(),
    };
    assert_eq!(variant.camel_case_name(), "freeStagingRelease");
}
