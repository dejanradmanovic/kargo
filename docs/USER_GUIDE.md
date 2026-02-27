# Kargo User Guide

A comprehensive guide to Kargo, a Cargo-inspired build tool for Kotlin written in Rust.

---

## 1. Introduction

**Kargo** is a fast, modern build and dependency management tool for Kotlin. Inspired by [Cargo](https://doc.rust-lang.org/cargo/) (Rust's package manager), it brings a familiar workflow, declarative configuration, and first-class support for:

- **Kotlin/JVM** — Single-target JVM applications and libraries
- **Kotlin Multiplatform (KMP)** — Shared code across JVM, iOS, JS, Wasm, and native targets
- **Compose Multiplatform** — UI toolkit for cross-platform apps
- **Version catalogs** — Centralized dependency version management

### Why Kargo?

- **Declarative** — A single `Kargo.toml` describes the entire project
- **Fast** — Written in Rust, optimized for speed and incremental builds
- **Simple** — No Groovy DSL or complex build scripts
- **Compatible** — Uses Maven repositories and coordinates; works with existing Kotlin ecosystems

---

## 2. Installation

### Building from Source

Clone the repository and install the CLI:

```bash
git clone https://github.com/VicertDev/kargo.git
cd kargo
cargo install --path crates/kargo-cli
```

Ensure `~/.cargo/bin` is in your `PATH`. Verify:

```bash
kargo --version
```

### Future Plans

Binary releases (pre-built binaries for Linux, macOS, Windows) are planned for future releases.

---

## 3. Quick Start

### Create a New Project

```bash
kargo new my-app
cd my-app
```

This creates:

```
my-app/
├── Kargo.toml
├── Kargo.lock
├── .gitignore
├── .kargo.env
└── src/
    ├── main/kotlin/Main.kt
    └── test/kotlin/
```

### Project Structure

- **`Kargo.toml`** — Project manifest (name, version, dependencies, targets)
- **`src/main/kotlin/`** — Main source code
- **`src/test/kotlin/`** — Test source code

### Build and Run

```bash
kargo build
kargo run
```

`kargo run` compiles and executes the JVM application’s `main` function.

---

## 4. Project Templates

Use the `--template` (or `-t`) flag with `kargo new` or `kargo init`:

| Template | Description |
|----------|-------------|
| **jvm** (default) | JVM-only application with `Main.kt` |
| **kmp** | Kotlin Multiplatform with `commonMain`, `jvmMain`, `iosMain` |
| **compose** | KMP + Compose Multiplatform UI |
| **lib** | Library project (JVM target) |
| **cli** | JVM-only CLI-style application |

### Examples

```bash
# JVM app (default)
kargo new my-app

# KMP with iOS and JVM
kargo new my-kmp --template kmp

# Compose Multiplatform
kargo new my-compose-app --template compose

# Library
kargo new my-lib --template lib

# Initialize in current directory
kargo init --template cli
```

---

## 5. Project Structure

### JVM-Only Layout

```
project/
├── Kargo.toml
└── src/
    ├── main/kotlin/       # Production code
    └── test/kotlin/       # Test code
```

### KMP Layout

```
project/
├── Kargo.toml
└── src/
    ├── commonMain/kotlin/     # Shared code
    ├── commonMain/resources/
    ├── commonTest/kotlin/
    ├── jvmMain/kotlin/
    ├── jvmTest/kotlin/
    ├── iosMain/kotlin/
    ├── iosTest/kotlin/
    └── ... (other targets: js, wasm-js, etc.)
```

### With Flavors

When using `[flavors]`, variant-specific source sets can live under `src/<variant>/` (e.g. `src/paid-staging-main/kotlin`). Build outputs go to `build/<variant>/<target>/`.

### Key Files

| File | Purpose |
|------|---------|
| **Kargo.toml** | Project manifest (required) |
| **Kargo.lock** | Locked dependency versions (commit for applications) |
| **.kargo.env** | Build secrets and credentials (gitignored) |

---

## 6. Configuration Reference

The `Kargo.toml` manifest uses TOML. Below is a full reference of supported sections.

### 6.1 `[package]`

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Package name (alphanumeric, hyphens) |
| `version` | Yes | SemVer (e.g. `1.0.0`) |
| `kotlin` | Yes | Kotlin compiler version |
| `description` | No | Short description |
| `authors` | No | List of authors |
| `license` | No | SPDX identifier (e.g. `MIT`, `Apache-2.0`) |
| `repository` | No | Source repository URL |

```toml
[package]
name = "my-app"
version = "0.1.0"
kotlin = "2.3.0"
description = "My Kotlin application"
authors = ["Jane Doe <jane@example.com>"]
license = "MIT"
repository = "https://github.com/user/my-app"
```

### 6.2 `[targets]` / `[targets.<name>]`

Define compilation targets. Single target:

```toml
[targets.jvm]
java-target = "21"
```

Multiple targets (KMP):

```toml
[targets]
jvm = { java-target = "21" }
ios-arm64 = {}
ios-simulator-arm64 = {}
js = { module-kind = "es" }
wasm-js = {}
```

Target options:

| Option | Targets | Description |
|--------|---------|-------------|
| `java-target` | jvm | JVM bytecode target (e.g. `"17"`, `"21"`) |
| `module-kind` | js | `"es"` or `"commonjs"` |
| `cinterop` | native | C interop definitions (see [target.rs](crates/kargo-core/src/target.rs)) |

### 6.3 `[compose]`

Enable Compose Multiplatform:

```toml
[compose]
enabled = true
```

### 6.4 `[dependencies]` and `[dev-dependencies]`

**Shorthand** (Maven coordinate):

```toml
[dependencies]
kotlinx-coroutines = "org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0"
kotlin-test = { group = "org.jetbrains.kotlin", artifact = "kotlin-test", version = "2.3.0" }

[dev-dependencies]
kotlin-test = "org.jetbrains.kotlin:kotlin-test:2.3.0"
```

**Detailed form**:

```toml
some-lib = { group = "com.example", artifact = "my-lib", version = "1.0.0" }
optional-lib = { group = "com.example", artifact = "opt", version = "1.0", optional = true }
scoped = { group = "com.example", artifact = "x", version = "1.0", scope = "runtime" }
```

**Catalog reference**:

```toml
kotlinx-coroutines = { catalog = "libs", bundle = true }
```

### 6.5 `[target.<name>.dependencies]`

Target-specific dependencies:

```toml
[target.jvm.dependencies]
ktor-server = "io.ktor:ktor-server-core:2.3.0"
```

### 6.6 `[flavor.<name>.dependencies]`

Flavor-specific dependencies:

```toml
[flavor.free.dependencies]
ad-sdk = "com.example:ad-sdk:2.0.0"

[flavor.paid.dependencies]
premium-features = "com.example:premium:1.0.0"
```

### 6.7 `[plugins]`

Kotlin compiler plugins:

```toml
[plugins]
serialization = "org.jetbrains.kotlin.plugin.serialization"
serialization-detailed = { id = "org.jetbrains.kotlin.plugin.serialization", version = "2.3.0" }
```

### 6.8 `[flavors]`

Build flavors with dimensions, defaults, and excludes:

```toml
[flavors]
dimensions = ["tier", "environment"]
default = { tier = "paid", environment = "staging" }
# Exclude specific variant combinations
exclude = [
  { tier = "free", environment = "production" }
]

[flavors.tier.free]
build-config = { IS_PAID = "false", AD_SUPPORTED = "true" }
application-id-suffix = ".free"

[flavors.tier.paid]
build-config = { IS_PAID = "true", AD_SUPPORTED = "false" }

[flavors.environment.staging]
build-config = { API_URL = "https://staging.api.example.com" }

[flavors.environment.production]
build-config = { API_URL = "https://api.example.com" }
```

### 6.9 `[hooks]`

Run commands before/after build phases:

```toml
[hooks]
pre-build = ["fmt --check", "lint"]
post-compile = ["generate-docs"]
post-test = ["coverage-report"]
```

### 6.10 `[lint]` and `[format]`

```toml
[lint]
rules = ["naming", "style", "complexity"]
severity = "warning"

[format]
style = "official"
indent = 4
max-line-length = 120
```

### 6.11 `[profile.dev]` and `[profile.release]`

```toml
[profile.dev]
debug = true
optimization = false

[profile.release]
debug = false
optimization = true
compiler-args = ["-Xopt-in=kotlin.RequiresOptIn"]
```

### 6.12 `[repositories]`

Custom Maven repositories:

```toml
[repositories]
central = "https://repo.maven.apache.org/maven2"
my-private = { url = "https://nexus.company.com/maven", username = "${env:NEXUS_USER}", password = "${env:NEXUS_PASS}" }
```

### 6.13 `[workspace]`

Multi-module workspace:

```toml
[workspace]
members = ["app", "shared", "libs/*"]
exclude = ["experimental"]
```

### 6.14 `[toolchain]`

Project-level toolchain overrides:

```toml
[toolchain]
jdk = "21"
kotlin-mirror = "https://mirror.example.com/kotlin"
auto-download = true
```

### 6.15 `[catalog]`

Version catalog (Gradle-style):

```toml
[catalog.versions]
kotlin = "2.3.0"
coroutines = "1.8.0"

[catalog.libraries]
kotlinx-coroutines = { group = "org.jetbrains.kotlinx", artifact = "kotlinx-coroutines-core", version.ref = "coroutines" }
ktor-server-core = { group = "io.ktor", artifact = "ktor-server-core", version = "2.3.0" }

[catalog.bundles]
ktor-server = ["ktor-server-core"]

[catalog.plugins]
serialization = { id = "org.jetbrains.kotlin.plugin.serialization", version.ref = "kotlin" }
```

### 6.16 `[test.coverage]`

```toml
[test.coverage]
engine = "jacoco"
min-line = 80
min-branch = 70
exclude = ["**/generated/**"]
```

### 6.17 `[signing]`

Publishing signatures:

```toml
[signing]
gpg-key = "ABCD1234"
gpg-password = "${env:GPG_PASSPHRASE}"
```

### 6.18 `[package.docker]`

Docker image configuration for `kargo package --docker`:

```toml
[package.docker]
base-image = "eclipse-temurin:21-jre"
ports = [8080]
entrypoint = ["java", "-jar", "/app/app.jar"]
```

---

## 7. Dependencies

### Add a Dependency

```bash
# Main dependency
kargo add org.jetbrains.kotlinx:kotlinx-coroutines-core:1.8.0

# Dev dependency
kargo add org.jetbrains.kotlin:kotlin-test:2.3.0 --dev

# Target-specific
kargo add io.ktor:ktor-server-core:2.3.0 --target jvm

# Flavor-specific
kargo add com.example:ad-sdk:2.0.0 --flavor free
```

### Remove a Dependency

```bash
kargo remove kotlinx-coroutines
```

### Update Dependencies

```bash
kargo update
```

### View Dependency Tree

```bash
kargo tree
kargo tree --depth 2
kargo tree --duplicates
kargo tree --inverted
kargo tree --why kotlinx-coroutines
kargo tree --conflicts
kargo tree --licenses
```

### Fetch Without Building

```bash
kargo fetch
```

### Lockfile Management

```bash
kargo lock
```

### Outdated Dependencies

```bash
kargo outdated
kargo outdated --major
```

### Vulnerability Scanning

```bash
kargo audit
```

---

## 8. Building

### Build

```bash
kargo build
kargo build --target jvm
kargo build --profile release
kargo build --release
kargo build --flavor paid
kargo build --variant paid-staging-release
kargo build --all-variants
kargo build --offline
kargo build --timings
```

| Flag | Description |
|------|-------------|
| `-t, --target` | Build specific target (jvm, ios-arm64, js, etc.) |
| `-p, --profile` | Profile (dev, release) |
| `--release` | Same as `--profile release` |
| `--flavor` | Flavor name (single dimension or composite) |
| `--variant` | Full variant (e.g. `free-staging-dev`) |
| `--all-variants` | Build all flavor×profile combinations |
| `--offline` | Use cached dependencies only |
| `--timings` | Print build timing report |

### Run

```bash
kargo run
kargo run --target jvm
kargo run --variant paid-release
kargo run -- arg1 arg2
```

### Check

Type-check without producing artifacts:

```bash
kargo check
kargo check --variant release
```

### Clean

```bash
kargo clean
kargo clean --variant paid-staging
```

---

## 9. Build Flavors and Variants

### Dimensions and Flavors

**Dimensions** define axes (e.g. `tier`, `environment`). **Flavors** are values per dimension (e.g. `free`, `paid`, `staging`, `production`).

A **variant** is one flavor per dimension plus a profile: `free-staging-release`.

### How Variants Are Computed

- Cartesian product of flavor dimensions × profiles
- `exclude` removes unwanted combinations
- `default` selects the variant when none is specified

### Flavor-Specific Source Sets

Source under `src/<variant>/` is included only for that variant (e.g. `src/paidMain/kotlin`).

### BuildConfig Generation

`build-config` entries in flavor definitions become compile-time constants and environment variables (`KARGO_BUILD_CONFIG_*`).

### Default Variant

When `--variant` and `--flavor` are omitted, Kargo uses `default` from `[flavors]` plus the default profile (usually `dev`).

### Variant Filtering

```bash
kargo variant list
kargo variant info paid-staging-release
```

---

## 10. Testing and Coverage

### Run Tests

```bash
kargo test
kargo test --target jvm
kargo test --filter "MyTest"
kargo test --flavor free
kargo test --variant paid-staging
kargo test --parallel
kargo test --coverage
kargo test --report junit,html
```

### Coverage Configuration

Configure in `Kargo.toml`:

```toml
[test.coverage]
engine = "jacoco"
min-line = 80
min-branch = 70
exclude = ["**/generated/**", "**/build/**"]
```

### Benchmarking

```bash
kargo bench
kargo bench --compare baseline-name
```

---

## 11. Linting and Formatting

### Lint

```bash
kargo lint
kargo lint --fix
```

### Format

```bash
kargo fmt
kargo fmt --check
```

### Auto-Fix

Apply all auto-fixable suggestions:

```bash
kargo fix
```

### Configuring Rules

In `Kargo.toml`:

```toml
[lint]
rules = ["naming", "style", "complexity", "performance", "correctness"]
severity = "warning"
```

### Built-in Rule Categories

| Category | Description |
|----------|-------------|
| **naming** | Class/function/variable naming conventions |
| **style** | Braces, spacing, import ordering |
| **complexity** | Function length, nesting, cyclomatic complexity |
| **performance** | Allocations, deprecated API usage |
| **correctness** | Unreachable code, unused variables |

---

## 12. Plugins and Hooks

### Hook System

Hooks run commands at build phases:

| Hook | When |
|------|------|
| `pre-build` | Before compilation |
| `post-compile` | After successful compile |
| `post-test` | After tests complete |

```toml
[hooks]
pre-build = ["fmt --check", "lint"]
post-test = ["coverage-report"]
```

### Plugin Tiers

1. **Subcommand plugins** — Extend `kargo` with new commands
2. **Rhai scripts** — Embedded scripting for custom logic
3. **WASM extensions** (future) — Sandboxed WASM plugins

### Plugin Management

```bash
kargo plugin install my-plugin
kargo plugin list
kargo plugin remove my-plugin
```

---

## 13. Environment Secrets

### .kargo.env

Build secrets and credentials (private registry auth, signing passwords, CI tokens) live in `.kargo.env`. This file is gitignored and never baked into the application. Shell-style `KEY=value` format.

```bash
NEXUS_USERNAME=deploy
NEXUS_PASSWORD=s3cret
MAVEN_TOKEN=abc123
KEYSTORE_PASSWORD=changeit
ANDROID_SDK=/Users/jane/Library/Android/sdk
```

### Interpolation in Kargo.toml

Reference `.kargo.env` values (or process env vars) via `${env:VAR}`:

```toml
[repositories]
my-private = {
  url = "https://nexus.company.com/maven",
  username = "${env:NEXUS_USERNAME}",
  password = "${env:NEXUS_PASSWORD}"
}
```

Build config values that should be baked into the app go directly in `Kargo.toml`:

```toml
[flavors.environment.production]
build-config = {
  MAPS_KEY = "AIzaSyB...",
  API_URL = "https://api.production.example.com"
}
```

### Security Rules

- `kargo new` / `kargo init` add `.kargo.env` to `.gitignore`
- `kargo publish` fails if `Kargo.toml` has unresolved `${env:...}` placeholders

### kargo env

```bash
kargo env          # values masked
kargo env --reveal # values shown
```

Prints `.kargo.env` entries. Values are masked by default; use `--reveal` to show them.

---

## 14. Toolchain Management

### Commands

```bash
kargo toolchain install 2.3.0
kargo toolchain list
kargo toolchain remove 2.3.0
kargo toolchain use 2.3.0
kargo toolchain path
```

### Auto-Download

When `auto-download = true` (default), Kargo downloads Kotlin when needed.

### SDK Discovery

Kargo discovers:
- **JDK** — From `JAVA_HOME`, `.kargo.env`, or common install paths
- **Xcode** — For iOS/macOS targets (when on macOS)
- **Android SDK** — From `ANDROID_HOME`, `ANDROID_SDK_ROOT`, or `.kargo.env`

---

## 15. Publishing

### Publish

```bash
kargo publish
```

### Login

```bash
kargo login
```

### Package

Create distributable artifacts:

```bash
kargo package
kargo package --docker
kargo package --ios-universal
```

### Artifact Signing

Configure `[signing]` and use GPG for published artifacts.

---

## 16. Workspace Support

### Multi-Module Layout

```
workspace/
├── Kargo.toml          # Root manifest with [workspace]
├── app/                # Application module
│   └── Kargo.toml
└── shared/             # Shared library
    └── Kargo.toml
```

### [workspace] Config

```toml
[workspace]
members = ["app", "shared", "libs/*"]
exclude = ["experimental"]
```

### Version Catalogs

Place a `[catalog]` in the root `Kargo.toml`; members can reference it via `{ catalog = "root", bundle = "..." }` or version refs.

---

## 17. IDE Integration

### kargo metadata

Emit machine-readable project metadata for IDE/editors:

```bash
kargo metadata --format json
```

### kargo lsp

Start the Language Server Protocol server for Kotlin:

```bash
kargo lsp
```

---

## 18. Global Configuration

Config file: `~/.kargo/config.toml`

```toml
[build]
jobs = 8
default-target = "jvm"

[cache]
dir = "~/.kargo/cache"
max-size = "5GB"
remote = "https://cache.example.com"
remote-auth = "bearer-token"
remote-push = true

[repositories]
# Add global repository overrides

[credentials]
nexus = { username = "user", password = "pass" }
nexus-token = { token-cmd = "secret-tool get nexus" }

[toolchain]
kotlin-mirror = "https://mirror.example.com/kotlin"
auto-download = true
jdk = "/usr/lib/jvm/java-21"

[lint]
default-rules = ["naming", "style"]

[format]
style = "official"
```

---

## 19. Environment Variables

Kargo sets these during builds, hooks, and plugins:

### Package Variables

| Variable | Example | Description |
|----------|---------|-------------|
| `KARGO_MANIFEST_DIR` | `/home/user/my-app` | Directory containing `Kargo.toml` |
| `KARGO_PKG_NAME` | `my-app` | Package name |
| `KARGO_PKG_VERSION` | `1.0.0` | Package version |
| `KARGO_PKG_VERSION_MAJOR` | `1` | Major version |
| `KARGO_PKG_VERSION_MINOR` | `0` | Minor version |
| `KARGO_PKG_VERSION_PATCH` | `0` | Patch version |
| `KARGO_PKG_AUTHORS` | `Jane Doe` | Authors |
| `KARGO_PKG_DESCRIPTION` | `My Kotlin app` | Description |
| `KARGO_PKG_REPOSITORY` | `https://github.com/...` | Repository URL |

### Build Context Variables

| Variable | Example | Description |
|----------|---------|-------------|
| `KARGO_BUILD_DIR` | `/home/user/my-app/build` | Build output root |
| `KARGO_TARGET` | `jvm` | Current compilation target |
| `KARGO_PROFILE` | `release` | Active build profile |
| `KARGO_JOBS` | `8` | Parallel jobs |
| `KARGO_KOTLIN_VERSION` | `2.3.0` | Kotlin compiler version |
| `KARGO_TOOLCHAIN_DIR` | `~/.kargo/toolchains/kotlin-2.3.0` | Active toolchain path |
| `KARGO_CACHE_DIR` | `~/.kargo/cache` | Dependency cache |

### Flavor/Variant Variables (when flavors exist)

| Variable | Example | Description |
|----------|---------|-------------|
| `KARGO_VARIANT` | `paidProductionRelease` | Full variant name |
| `KARGO_FLAVOR_<DIM>` | `KARGO_FLAVOR_TIER=paid` | Value per dimension |
| `KARGO_BUILD_CONFIG_*` | `KARGO_BUILD_CONFIG_API_URL=...` | Build-config entries as env vars |

### Workspace Variables (multi-module)

| Variable | Example | Description |
|----------|---------|-------------|
| `KARGO_WORKSPACE_DIR` | `/home/user/my-workspace` | Workspace root |
| `KARGO_WORKSPACE_MEMBER` | `shared` | Current member being built |

### Environment Variables from `.kargo.env`

All entries in `.kargo.env` are loaded as environment variables during builds, hooks, and plugin execution. They are also available via `${env:VAR}` interpolation in `Kargo.toml`.

---

## Additional Commands Reference

| Command | Description |
|---------|-------------|
| `kargo doc [--open]` | Generate KDoc documentation |
| `kargo watch [-c command]` | Rebuild on file changes |
| `kargo repl` | Launch Kotlin REPL |
| `kargo script <file>` | Run a Kotlin script |
| `kargo completions <shell>` | Generate shell completions |
| `kargo self update` | Update Kargo |
| `kargo self info` | Version, config paths, cache size |
| `kargo self clean` | Clean global caches |
| `kargo cache stats` | Cache hit/miss and size |
| `kargo cache clean` | Clear local build cache |
| `kargo doctor` | Diagnose project health |
| `kargo migrate` | Migrate from Gradle |
