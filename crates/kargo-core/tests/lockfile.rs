use kargo_core::lockfile::{LockedDependencyRef, LockedPackage, Lockfile};

#[test]
fn round_trip_serialize_deserialize() {
    let lockfile = Lockfile {
        package: vec![LockedPackage {
            name: "kotlin-stdlib".to_string(),
            group: "org.jetbrains.kotlin".to_string(),
            version: "1.9.0".to_string(),
            checksum: Some("abc123".to_string()),
            source: Some("maven".to_string()),
            dependencies: vec![LockedDependencyRef {
                name: "annotations".to_string(),
                group: "org.jetbrains".to_string(),
                version: "24.0.0".to_string(),
            }],
        }],
    };

    let serialized = lockfile.to_string_pretty().unwrap();
    let deserialized: Lockfile = toml::from_str(&serialized).unwrap();

    assert_eq!(deserialized.package.len(), 1);
    assert_eq!(deserialized.package[0].name, lockfile.package[0].name);
    assert_eq!(deserialized.package[0].group, lockfile.package[0].group);
    assert_eq!(deserialized.package[0].version, lockfile.package[0].version);
    assert_eq!(
        deserialized.package[0].checksum,
        lockfile.package[0].checksum
    );
    assert_eq!(deserialized.package[0].source, lockfile.package[0].source);
    assert_eq!(
        deserialized.package[0].dependencies.len(),
        lockfile.package[0].dependencies.len()
    );
}

#[test]
fn lockfile_empty_packages_serializes_deserializes() {
    let lockfile = Lockfile { package: vec![] };
    let serialized = lockfile.to_string_pretty().unwrap();
    let deserialized: Lockfile = toml::from_str(&serialized).unwrap();
    assert!(deserialized.package.is_empty());
}
