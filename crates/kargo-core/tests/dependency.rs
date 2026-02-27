use kargo_core::dependency::{DependencyScope, MavenCoordinate};

#[test]
fn maven_coordinate_parse_valid() {
    let coord = MavenCoordinate::parse("com.example:my-lib:1.0.0").unwrap();
    assert_eq!(coord.group_id, "com.example");
    assert_eq!(coord.artifact_id, "my-lib");
    assert_eq!(coord.version, "1.0.0");
}

#[test]
fn maven_coordinate_parse_two_parts_returns_none() {
    assert!(MavenCoordinate::parse("group:artifact").is_none());
}

#[test]
fn maven_coordinate_parse_empty_string() {
    assert!(MavenCoordinate::parse("").is_none());
}

#[test]
fn maven_coordinate_parse_four_parts_returns_none() {
    assert!(MavenCoordinate::parse("group:artifact:version:extra").is_none());
}

#[test]
fn maven_coordinate_display_roundtrip() {
    let s = "com.example:my-lib:1.0.0";
    let coord = MavenCoordinate::parse(s).unwrap();
    assert_eq!(coord.to_string(), s);
}

#[test]
fn dependency_scope_default_is_compile() {
    assert_eq!(DependencyScope::default(), DependencyScope::Compile);
}
