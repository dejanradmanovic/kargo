# Contributor Guide

Welcome! Kargo is a Cargo-inspired build tool for Kotlin written in Rust. We're glad you're interested in contributing. This guide will help you get set up and understand the project.

## Prerequisites

- **Rust 1.80+** — The project uses `rust-version = "1.80"`. Install via [rustup](https://rustup.rs/).
- **cargo** — Comes with the Rust toolchain.
- **Kotlin (optional)** — Useful for end-to-end testing (e.g., `kotlinc` availability), but not required for most development.

Verify your setup:

```bash
rustc --version   # should be 1.80 or newer
cargo --version
```

## Getting Started

1. **Clone the repository**
   ```bash
   git clone https://github.com/VicertDev/kargo.git
   cd kargo
   ```

2. **Build the project**
   ```bash
   cargo build
   ```

3. **Run all tests**
   ```bash
   cargo test --workspace
   ```

4. **Run the CLI**
   ```bash
   cargo run -- <command>
   ```

## Project Structure

Kargo is a Rust workspace with 10 crates. Each has a focused role:

| Crate | Role |
|-------|------|
| `kargo-cli` | Binary entry point; clap CLI parsing and command dispatch |
| `kargo-ops` | High-level operations wiring CLI to subsystems (build, run, test, add, publish, etc.) |
| `kargo-core` | Core data types: Manifest, Package, Workspace, Target, SourceSet, Dependency, Flavor, Variant, Profile, Config, Lockfile, VersionCatalog, Properties |
| `kargo-resolver` | Dependency resolution (Maven nearest-wins algorithm) |
| `kargo-maven` | Maven repository protocol (POM parsing, metadata, artifact download, cache) |
| `kargo-compiler` | Kotlin compiler orchestration (kotlinc, Compose, KSP/KAPT, incremental builds) |
| `kargo-plugin` | Plugin system (Rhai scripting, subcommand discovery, hooks) |
| `kargo-lint` | Lint engine and formatter (tree-sitter, rules, SARIF output) |
| `kargo-toolchain` | Kotlin toolchain management (compiler download, JDK/SDK detection) |
| `kargo-util` | Shared utilities (errors, FS, hashing, process spawning, progress UI) |

## Dependency Direction Rules

Crates follow a strict layering. **Never add dependencies from `kargo-core` to any crate except `kargo-util`.** Core stays dependency-light.

Dependency graph:

```
kargo-util (base)
    ├── kargo-core
    ├── kargo-toolchain
    │
    ├── kargo-maven ───────► kargo-core
    ├── kargo-plugin ──────► kargo-core
    ├── kargo-lint ────────► kargo-core
    │
    ├── kargo-resolver ───► kargo-core, kargo-maven
    ├── kargo-compiler ───► kargo-core, kargo-toolchain
    │
    ├── kargo-ops ────────► kargo-core, kargo-resolver, kargo-maven,
    │                      kargo-compiler, kargo-plugin, kargo-lint,
    │                      kargo-toolchain
    │
    └── kargo-cli ───────► kargo-ops, kargo-core, kargo-util
```

**Rule:** `kargo-core` depends only on `kargo-util`. Do not introduce `kargo-core` → `kargo-ops`, `kargo-maven`, etc.

## Code Conventions

See [`.cursor/rules/rust-conventions.mdc`](../.cursor/rules/rust-conventions.mdc) for the full rules. Summary:

- **Error handling:** Use `thiserror` for error types, `miette` for user-facing reporting. No `.unwrap()` in library code; use `?` or return `Result`. `.expect()` is acceptable only in `main()` setup in `kargo-cli`.
- **Logging:** Use `tracing` macros (`tracing::info!`, `tracing::debug!`, etc.). No `println!`/`eprintln!` in library code.
- **API design:** Prefer `&str` over `String`, `&Path` over `PathBuf` in parameters. Return owned types.
- **Types:** Use `BTreeMap` for deterministic ordering (manifests, lockfiles). Derive `Debug`, `Clone`, `Serialize`, `Deserialize` on public types.
- **TOML fields:** Use `#[serde(rename_all = "kebab-case")]` for Kargo.toml conventions.

## Testing Conventions

See [`.cursor/rules/testing.mdc`](../.cursor/rules/testing.mdc) for details.

- **Unit tests:** Inline in source files under `#[cfg(test)] mod tests { ... }`
- **Integration tests:** In `crates/<crate>/tests/` and workspace `tests/` (with fixtures in `tests/fixtures/`)
- **Test naming:** `test_<what>_<scenario>` (e.g., `test_parse_manifest_with_flavors`)
- **Assertions:** Use `assert_eq!(actual, expected, "message")` with descriptive messages
- **Run tests:** `cargo test --workspace`

## Commit Message Format

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <description>
```

**Types:** `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`

**Scopes:** `cli`, `core`, `ops`, `maven`, `resolver`, `compiler`, `plugin`, `lint`, `toolchain`, `util`

**Examples:**

```
feat(resolver): add nearest-wins conflict resolution
fix(core): handle empty source-set in manifest parsing
docs(util): document progress API
test(maven): add POM metadata parsing tests
chore(deps): bump tokio to 1.x
```

## Pull Request Process

1. **Create a branch** — Use a descriptive branch name (e.g., `feat/resolver-nearest-wins` or `fix/core-empty-sourceset`).
2. **Make your changes** — Follow the code and testing conventions.
3. **Ensure tests pass** — Run `cargo test --workspace`.
4. **Submit a PR** — Describe your changes clearly. Reference any related issues.
5. **Address review feedback** — Maintainers may request updates before merging.

## Development Tips

- **Run a single crate’s tests:**
  ```bash
  cargo test -p kargo-core
  cargo test -p kargo-resolver
  ```

- **Check formatting:**
  ```bash
  cargo fmt --check
  ```

- **Run clippy:**
  ```bash
  cargo clippy --workspace
  ```

- **Build release binary:**
  ```bash
  cargo build --release
  # Binary: target/release/kargo
  ```

- **Run CLI with verbose logging:**
  ```bash
  RUST_LOG=debug cargo run -- <command>
  ```

---

Thank you for contributing to Kargo!
