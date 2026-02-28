use kargo_core::properties::{interpolate, load_env_file};
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
fn test_load_env_from_fixture() {
    let path = fixtures_dir().join("test.kargo.env");
    let env_vars = load_env_file(&path).unwrap();

    assert_eq!(env_vars.get("MAPS_API_KEY").unwrap(), "AIzaSyBTestKey456");
    assert_eq!(
        env_vars.get("SENTRY_DSN").unwrap(),
        "https://def@sentry.io/456"
    );
    assert_eq!(
        env_vars.get("DATABASE_URL").unwrap(),
        "postgres://localhost/testdb"
    );
    assert_eq!(env_vars.get("NEXUS_TOKEN").unwrap(), "test-token-abc123");
}

#[test]
fn test_interpolate_with_fixture_env() {
    let path = fixtures_dir().join("test.kargo.env");
    let env_overrides = load_env_file(&path).unwrap();

    let input = "key=${env:MAPS_API_KEY} dsn=${env:SENTRY_DSN}";
    let result = interpolate(input, &env_overrides);
    assert_eq!(
        result,
        "key=AIzaSyBTestKey456 dsn=https://def@sentry.io/456"
    );
}

#[test]
fn test_interpolate_mixed_env_refs() {
    let path = fixtures_dir().join("test.kargo.env");
    let env_overrides = load_env_file(&path).unwrap();

    let input = "db=${env:DATABASE_URL} token=${env:NEXUS_TOKEN}";
    let result = interpolate(input, &env_overrides);
    assert_eq!(
        result,
        "db=postgres://localhost/testdb token=test-token-abc123"
    );
}
