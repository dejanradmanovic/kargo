use kargo_util::process::CommandBuilder;

#[test]
fn test_builder_simple_command() {
    let output = CommandBuilder::new("echo").arg("hello").exec().unwrap();
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

    // Write a marker file and verify the command can see it from the cwd.
    // This avoids path comparison issues on Windows (8.3 short names, UNC prefixes).
    let marker = tmp.path().join("kargo_cwd_test.marker");
    std::fs::write(&marker, "ok").unwrap();

    #[cfg(unix)]
    let output = CommandBuilder::new("ls")
        .arg("kargo_cwd_test.marker")
        .cwd(tmp.path().to_str().unwrap())
        .exec()
        .unwrap();

    #[cfg(windows)]
    let output = CommandBuilder::new("cmd")
        .args(["/C", "dir", "/b", "kargo_cwd_test.marker"])
        .cwd(tmp.path().to_str().unwrap())
        .exec()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.trim().contains("kargo_cwd_test.marker"));
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
