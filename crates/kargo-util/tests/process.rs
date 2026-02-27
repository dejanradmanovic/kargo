use kargo_util::process::CommandBuilder;

#[test]
fn test_builder_simple_command() {
    let output = CommandBuilder::new("echo")
        .arg("hello")
        .exec()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "hello");
}

#[test]
fn test_builder_multiple_args() {
    let output = CommandBuilder::new("echo")
        .args(["one", "two", "three"])
        .exec()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "one two three");
}

#[test]
fn test_builder_with_env() {
    let output = CommandBuilder::new("sh")
        .arg("-c")
        .arg("echo $MY_TEST_VAR")
        .env("MY_TEST_VAR", "kargo_test_value")
        .exec()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "kargo_test_value");
}

#[test]
fn test_builder_with_cwd() {
    let tmp = tempfile::TempDir::new().unwrap();
    let output = CommandBuilder::new("pwd")
        .cwd(tmp.path().to_str().unwrap())
        .exec()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        std::path::PathBuf::from(stdout.trim()),
        tmp.path().canonicalize().unwrap()
    );
}

#[test]
fn test_builder_nonexistent_program() {
    let result = CommandBuilder::new("nonexistent_program_xyz_123").exec();
    assert!(result.is_err());
}

#[test]
fn test_builder_chaining() {
    let output = CommandBuilder::new("echo")
        .arg("a")
        .arg("b")
        .env("X", "Y")
        .exec()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "a b");
}
