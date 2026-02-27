use kargo_core::properties::{interpolate, load_env_file};
use std::collections::BTreeMap;
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn load_env_file_with_key_value_comments_blank_lines() {
    let mut tmp = NamedTempFile::new().unwrap();
    write!(
        tmp,
        "# comment line\n\
         KEY1=value1\n\
         \n\
         KEY2=value2\n\
         # another comment\n\
         KEY3  =  value3\n"
    )
    .unwrap();
    tmp.flush().unwrap();

    let env = load_env_file(tmp.path()).unwrap();
    assert_eq!(env.get("KEY1"), Some(&"value1".to_string()));
    assert_eq!(env.get("KEY2"), Some(&"value2".to_string()));
    assert_eq!(env.get("KEY3"), Some(&"value3".to_string()));
    assert_eq!(env.len(), 3);
}

#[test]
fn load_env_file_nonexistent_path_returns_empty_map() {
    let path = std::path::Path::new("/nonexistent/path/to/file.env");
    let env = load_env_file(path).unwrap();
    assert!(env.is_empty());
}

#[test]
fn interpolate_replaces_env_refs() {
    let mut env_overrides = BTreeMap::new();
    env_overrides.insert("HOME".to_string(), "/custom/home".to_string());

    let result = interpolate("path=${env:HOME}/file", &env_overrides);
    assert_eq!(result, "path=/custom/home/file");
}

#[test]
fn interpolate_missing_env_key_replaces_with_empty() {
    let env_overrides = BTreeMap::new();

    let result = interpolate("x=${env:NONEXISTENT_VAR_99999}", &env_overrides);
    assert_eq!(result, "x=");
}

#[test]
fn interpolate_no_placeholders_returns_input_unchanged() {
    let env_overrides = BTreeMap::new();

    let input = "plain text with no placeholders";
    let result = interpolate(input, &env_overrides);
    assert_eq!(result, input);
}

#[test]
fn interpolate_multiple_refs() {
    let mut env = BTreeMap::new();
    env.insert("USER".to_string(), "deploy".to_string());
    env.insert("PASS".to_string(), "s3cret".to_string());

    let result = interpolate("u=${env:USER} p=${env:PASS}", &env);
    assert_eq!(result, "u=deploy p=s3cret");
}
