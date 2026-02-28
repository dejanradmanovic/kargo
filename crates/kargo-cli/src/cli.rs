//! CLI argument definitions for Kargo.
//!
//! Uses `clap` derive macros to define the full command surface. Each command
//! corresponds to a handler in the [`super::commands`] module.

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "kargo",
    version,
    about = "A Cargo-inspired build tool for Kotlin",
    long_about = "Kargo is a fast, modern build and dependency management tool for Kotlin \
                  with first-class support for Kotlin Multiplatform and Compose Multiplatform."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Create a new Kargo project
    New {
        /// Project name
        name: String,
        /// Project template: jvm, kmp, cmp, android, lib
        #[arg(short, long, default_value = "jvm")]
        template: String,
    },

    /// Initialize Kargo in an existing directory
    Init {
        /// Project template: jvm, kmp, cmp, android, lib
        #[arg(short, long, default_value = "jvm")]
        template: String,
    },

    /// Build the project
    Build {
        /// Build target (e.g., jvm, ios-arm64, js)
        #[arg(short, long)]
        target: Option<String>,
        /// Build profile
        #[arg(short, long)]
        profile: Option<String>,
        /// Build in release mode
        #[arg(long)]
        release: bool,
        /// Build a specific flavor
        #[arg(long)]
        flavor: Option<String>,
        /// Build a specific variant
        #[arg(long)]
        variant: Option<String>,
        /// Build all variants
        #[arg(long)]
        all_variants: bool,
        /// Use only cached dependencies
        #[arg(long)]
        offline: bool,
        /// Print build timings
        #[arg(long)]
        timings: bool,
    },

    /// Build and run the project
    Run {
        /// Build target
        #[arg(short, long)]
        target: Option<String>,
        /// Build variant
        #[arg(long)]
        variant: Option<String>,
        /// Arguments to pass to the program
        #[arg(last = true)]
        args: Vec<String>,
    },

    /// Run tests
    Test {
        /// Build target
        #[arg(short, long)]
        target: Option<String>,
        /// Filter test names
        #[arg(short, long)]
        filter: Option<String>,
        /// Build flavor
        #[arg(long)]
        flavor: Option<String>,
        /// Build variant
        #[arg(long)]
        variant: Option<String>,
        /// Run tests in parallel
        #[arg(long)]
        parallel: bool,
        /// Enable code coverage
        #[arg(long)]
        coverage: bool,
        /// Report formats (e.g. junit,html)
        #[arg(long)]
        report: Option<String>,
    },

    /// Type-check without compiling
    Check {
        /// Build variant
        #[arg(long)]
        variant: Option<String>,
    },

    /// Remove build artifacts
    Clean {
        /// Clean specific variant only
        #[arg(long)]
        variant: Option<String>,
    },

    /// Add a dependency
    Add {
        /// Dependency coordinate (group:artifact:version)
        dep: String,
        /// Add as dev dependency
        #[arg(long)]
        dev: bool,
        /// Add to a specific target
        #[arg(long)]
        target: Option<String>,
        /// Add to a specific flavor
        #[arg(long)]
        flavor: Option<String>,
    },

    /// Remove a dependency
    #[command(alias = "rm")]
    Remove {
        /// Dependency name
        dep: String,
        /// Remove from dev dependencies
        #[arg(long)]
        dev: bool,
        /// Remove from a specific target
        #[arg(long)]
        target: Option<String>,
        /// Remove from a specific flavor
        #[arg(long)]
        flavor: Option<String>,
    },

    /// Update dependencies to latest compatible versions
    Update {
        /// Allow major version bumps
        #[arg(long)]
        major: bool,
        /// Update a specific dependency only
        #[arg(long)]
        dep: Option<String>,
        /// Show what would be updated without changing files
        #[arg(long)]
        dry_run: bool,
    },

    /// Download dependencies without building
    Fetch {
        /// Re-verify checksums of cached artifacts against the lockfile
        #[arg(long)]
        verify: bool,
    },

    /// Regenerate the lockfile
    Lock,

    /// Print the dependency tree
    Tree {
        /// Maximum depth
        #[arg(long)]
        depth: Option<u32>,
        /// Show duplicate dependencies
        #[arg(long)]
        duplicates: bool,
        /// Show inverted tree (dependents)
        #[arg(long)]
        inverted: bool,
        /// Explain why a dependency is included
        #[arg(long, rename_all = "kebab-case")]
        why: Option<String>,
        /// Show version conflicts
        #[arg(long)]
        conflicts: bool,
        /// Show dependency licenses
        #[arg(long)]
        licenses: bool,
    },

    /// Show outdated dependencies
    Outdated {
        /// Include major version bumps
        #[arg(long)]
        major: bool,
    },

    /// Scan dependencies for known vulnerabilities (OSV database)
    Audit {
        /// Minimum severity to fail on: low, moderate, high, critical
        #[arg(long)]
        fail_on: Option<String>,
    },

    /// Run the linter
    Lint {
        /// Auto-fix violations
        #[arg(long)]
        fix: bool,
    },

    /// Format source code
    Fmt {
        /// Check formatting without modifying files
        #[arg(long)]
        check: bool,
    },

    /// Auto-fix all suggestions
    Fix,

    /// Generate KDoc documentation
    Doc {
        /// Open in browser
        #[arg(long)]
        open: bool,
    },

    /// Run benchmarks
    Bench {
        /// Compare against a baseline
        #[arg(long)]
        compare: Option<String>,
    },

    /// Rebuild on file changes
    Watch {
        /// Command to run on changes
        #[arg(short, long, default_value = "build")]
        command: String,
    },

    /// Publish to a Maven repository
    Publish,

    /// Create a distributable package
    Package {
        /// Build a Docker image
        #[arg(long)]
        docker: bool,
        /// Build iOS universal framework
        #[arg(long)]
        ios_universal: bool,
    },

    /// Launch Kotlin REPL
    Repl,

    /// Run a Kotlin script
    Script {
        /// Script file path
        file: String,
    },

    /// Emit machine-readable project metadata
    Metadata {
        /// Output format
        #[arg(long, default_value = "json")]
        format: String,
    },

    /// Generate shell completions
    Completions {
        /// Shell type: bash, zsh, fish, powershell
        shell: String,
    },

    /// Manage plugins
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },

    /// Manage KMP targets
    Target {
        #[command(subcommand)]
        action: TargetAction,
    },

    /// Manage build variants
    Variant {
        #[command(subcommand)]
        action: VariantAction,
    },

    /// Manage build flavors
    Flavor {
        #[command(subcommand)]
        action: FlavorAction,
    },

    /// Manage Kotlin toolchains
    Toolchain {
        #[command(subcommand)]
        action: ToolchainAction,
    },

    /// Manage Kargo itself
    #[command(name = "self")]
    SelfCmd {
        #[command(subcommand)]
        action: SelfAction,
    },

    /// Manage build cache
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },

    /// Print resolved environment variables
    Env {
        /// Show secret values unmasked
        #[arg(long)]
        reveal: bool,
    },

    /// Diagnose project health
    Doctor,

    /// Migrate from a Gradle project
    Migrate,

    /// Start Language Server Protocol server
    Lsp,
}

#[derive(Subcommand, Debug)]
pub enum PluginAction {
    /// Install a plugin
    Install { name: String },
    /// List installed plugins
    List,
    /// Remove a plugin
    Remove { name: String },
}

#[derive(Subcommand, Debug)]
pub enum TargetAction {
    /// Add a KMP target
    Add { target: String },
    /// List available/active targets
    List,
    /// Remove a target
    Remove { target: String },
}

#[derive(Subcommand, Debug)]
pub enum VariantAction {
    /// List all build variants
    List,
    /// Show variant details
    Info { name: String },
}

#[derive(Subcommand, Debug)]
pub enum FlavorAction {
    /// Add a flavor to a dimension
    Add { dimension: String, name: String },
    /// Remove a flavor
    Remove { dimension: String, name: String },
}

#[derive(Subcommand, Debug)]
pub enum ToolchainAction {
    /// Download and install a Kotlin version (and optionally a JDK or Android SDK)
    Install {
        /// Kotlin version to install (e.g., 2.3.0)
        version: Option<String>,
        /// Install a JDK, optionally specifying the major version (e.g., --jdk 17)
        #[arg(long, num_args = 0..=1, default_missing_value = "21")]
        jdk: Option<String>,
        /// Install the Android SDK, optionally specifying the compile-sdk level (e.g., --android 34)
        #[arg(long, num_args = 0..=1, default_missing_value = "35")]
        android: Option<String>,
    },
    /// List installed toolchains
    List,
    /// Remove a cached toolchain, JDK, or Android SDK
    Remove {
        /// Kotlin version to remove (e.g., 2.3.0)
        version: Option<String>,
        /// Remove a managed JDK by major version (e.g., --jdk 21)
        #[arg(long)]
        jdk: Option<String>,
        /// Remove the managed Android SDK
        #[arg(long)]
        android: bool,
    },
    /// Set default Kotlin version
    Use { version: String },
    /// Print path to active toolchain
    Path,
}

#[derive(Subcommand, Debug)]
pub enum SelfAction {
    /// Update Kargo to the latest version
    Update {
        /// Only check for updates, don't install
        #[arg(long)]
        check: bool,
    },
    /// Show version, config paths, cache size
    Info,
    /// Clean global caches
    Clean,
}

#[derive(Subcommand, Debug)]
pub enum CacheAction {
    /// Show hit/miss rates and cache size
    Stats,
    /// Clear local build cache
    Clean,
    /// Push build outputs to remote cache
    Push,
}

pub fn parse() -> Cli {
    Cli::parse()
}
