use miette::Diagnostic;
use thiserror::Error;

/// Unified error type for all Kargo operations.
#[derive(Debug, Error, Diagnostic)]
pub enum KargoError {
    /// I/O operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Invalid or malformed manifest (e.g. Kargo.toml).
    #[error("Manifest error: {message}")]
    #[diagnostic(help("Check your Kargo.toml for syntax errors"))]
    Manifest { message: String },

    /// Dependency resolution failed (version conflicts, missing deps, etc.).
    #[error("Dependency resolution failed: {message}")]
    Resolution { message: String },

    /// Compilation of Kotlin or native code failed.
    #[error("Compilation failed: {message}")]
    Compilation { message: String },

    /// Network request or download failed.
    #[error("Network error: {message}")]
    Network { message: String },

    /// Toolchain (Kotlin/Java) discovery or configuration failed.
    #[error("Toolchain error: {message}")]
    Toolchain { message: String },

    /// Catch-all for miscellaneous errors.
    #[error("{message}")]
    Generic { message: String },
}

/// Convenience alias for `miette::Result<T>`.
pub type KargoResult<T> = miette::Result<T>;
