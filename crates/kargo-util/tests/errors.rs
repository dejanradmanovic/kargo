use kargo_util::errors::KargoError;

#[test]
fn test_io_error_display() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
    let err = KargoError::from(io_err);
    assert!(err.to_string().contains("I/O error"), "got: {err}");
}

#[test]
fn test_manifest_error_display() {
    let err = KargoError::Manifest {
        message: "bad syntax".to_string(),
    };
    assert_eq!(err.to_string(), "Manifest error: bad syntax");
}

#[test]
fn test_resolution_error_display() {
    let err = KargoError::Resolution {
        message: "conflict".to_string(),
    };
    assert_eq!(err.to_string(), "Dependency resolution failed: conflict");
}

#[test]
fn test_compilation_error_display() {
    let err = KargoError::Compilation {
        message: "kotlinc failed".to_string(),
    };
    assert_eq!(err.to_string(), "Compilation failed: kotlinc failed");
}

#[test]
fn test_network_error_display() {
    let err = KargoError::Network {
        message: "timeout".to_string(),
    };
    assert_eq!(err.to_string(), "Network error: timeout");
}

#[test]
fn test_toolchain_error_display() {
    let err = KargoError::Toolchain {
        message: "not found".to_string(),
    };
    assert_eq!(err.to_string(), "Toolchain error: not found");
}

#[test]
fn test_generic_error_display() {
    let err = KargoError::Generic {
        message: "something broke".to_string(),
    };
    assert_eq!(err.to_string(), "something broke");
}

#[test]
fn test_io_error_from_conversion() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
    let kargo_err: KargoError = io_err.into();
    matches!(kargo_err, KargoError::Io(_));
}
