use kargo_core::config::{dirs_path, GlobalConfig};

#[test]
fn test_global_config_default_jobs_nonzero() {
    let config = GlobalConfig::default();
    assert!(config.build.jobs > 0, "jobs should be > 0");
}

#[test]
fn test_global_config_default_no_target() {
    let config = GlobalConfig::default();
    assert!(config.build.default_target.is_none());
}

#[test]
fn test_global_config_default_cache_dir() {
    let config = GlobalConfig::default();
    assert_eq!(config.cache.dir, "~/.kargo/dependencies");
}

#[test]
fn test_global_config_auto_download_defaults_true_from_toml() {
    // When deserialized from an empty TOML, serde's default function kicks in
    let config: GlobalConfig = toml::from_str("").unwrap();
    assert!(config.toolchain.auto_download);
}

#[test]
fn test_global_config_default_empty_repos() {
    let config = GlobalConfig::default();
    assert!(config.repositories.is_empty());
}

#[test]
fn test_dirs_path_contains_kargo() {
    let path = dirs_path();
    assert!(path.ends_with(".kargo"));
}

#[test]
fn test_global_config_parse_from_toml() {
    let toml = r#"
[build]
jobs = 4
default-target = "jvm"

[cache]
dir = "/custom/cache"
max-size = "5GB"

[toolchain]
auto-download = false
jdk = "/usr/lib/jvm/java-21"
"#;
    let config: GlobalConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.build.jobs, 4);
    assert_eq!(config.build.default_target.as_deref(), Some("jvm"));
    assert_eq!(config.cache.dir, "/custom/cache");
    assert_eq!(config.cache.max_size.as_deref(), Some("5GB"));
    assert!(!config.toolchain.auto_download);
    assert_eq!(
        config.toolchain.jdk.as_deref(),
        Some("/usr/lib/jvm/java-21")
    );
}
