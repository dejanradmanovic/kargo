use serde::{Deserialize, Serialize};

/// Per-target configuration from `[targets.<name>]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    #[serde(default, rename = "java-target")]
    pub java_target: Option<String>,

    #[serde(default, rename = "module-kind")]
    pub module_kind: Option<String>,

    #[serde(default)]
    pub cinterop: Option<std::collections::BTreeMap<String, CInteropConfig>>,

    #[serde(default, rename = "min-sdk")]
    pub min_sdk: Option<u32>,

    #[serde(default, rename = "target-sdk")]
    pub target_sdk: Option<u32>,

    #[serde(default, rename = "compile-sdk")]
    pub compile_sdk: Option<u32>,
}

/// C/Objective-C interop configuration for Kotlin/Native targets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CInteropConfig {
    #[serde(rename = "def-file")]
    pub def_file: String,
    #[serde(default)]
    pub headers: Vec<String>,
    #[serde(default, rename = "compiler-opts")]
    pub compiler_opts: Vec<String>,
    #[serde(default, rename = "linker-opts")]
    pub linker_opts: Vec<String>,
}

/// All supported Kotlin compilation targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KotlinTarget {
    Jvm,
    /// JVM-based Android target (standard Android apps via Kotlin/JVM).
    Android,
    Js,
    WasmJs,
    WasmWasi,
    IosArm64,
    IosSimulatorArm64,
    IosX64,
    MacosArm64,
    MacosX64,
    LinuxX64,
    LinuxArm64,
    MingwX64,
    TvosArm64,
    TvosSimulatorArm64,
    WatchosArm64,
    WatchosSimulatorArm64,
    /// Kotlin/Native Android NDK target (ARM64).
    AndroidNativeArm64,
    /// Kotlin/Native Android NDK target (x86_64).
    AndroidNativeX64,
}

impl KotlinTarget {
    /// Parse a target name (kebab-case or camelCase) into a `KotlinTarget`.
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "jvm" => Some(Self::Jvm),
            "android" => Some(Self::Android),
            "js" => Some(Self::Js),
            "wasm-js" | "wasmJs" => Some(Self::WasmJs),
            "wasm-wasi" | "wasmWasi" => Some(Self::WasmWasi),
            "ios-arm64" | "iosArm64" => Some(Self::IosArm64),
            "ios-simulator-arm64" | "iosSimulatorArm64" => Some(Self::IosSimulatorArm64),
            "ios-x64" | "iosX64" => Some(Self::IosX64),
            "macos-arm64" | "macosArm64" => Some(Self::MacosArm64),
            "macos-x64" | "macosX64" => Some(Self::MacosX64),
            "linux-x64" | "linuxX64" => Some(Self::LinuxX64),
            "linux-arm64" | "linuxArm64" => Some(Self::LinuxArm64),
            "mingw-x64" | "mingwX64" => Some(Self::MingwX64),
            "tvos-arm64" | "tvosArm64" => Some(Self::TvosArm64),
            "tvos-simulator-arm64" | "tvosSimulatorArm64" => Some(Self::TvosSimulatorArm64),
            "watchos-arm64" | "watchosArm64" => Some(Self::WatchosArm64),
            "watchos-simulator-arm64" | "watchosSimulatorArm64" => {
                Some(Self::WatchosSimulatorArm64)
            }
            "android-native-arm64" | "androidNativeArm64" => Some(Self::AndroidNativeArm64),
            "android-native-x64" | "androidNativeX64" => Some(Self::AndroidNativeX64),
            _ => None,
        }
    }

    /// Returns `true` if this target compiles to native code (not JVM, JS, WASM, or Android).
    pub fn is_native(&self) -> bool {
        !matches!(
            self,
            Self::Jvm | Self::Android | Self::Js | Self::WasmJs | Self::WasmWasi
        )
    }

    /// Returns `true` if this is the Android JVM target.
    pub fn is_android(&self) -> bool {
        matches!(self, Self::Android)
    }

    /// Returns `true` if this target is an Apple platform (iOS, macOS, tvOS, watchOS).
    pub fn is_apple(&self) -> bool {
        matches!(
            self,
            Self::IosArm64
                | Self::IosSimulatorArm64
                | Self::IosX64
                | Self::MacosArm64
                | Self::MacosX64
                | Self::TvosArm64
                | Self::TvosSimulatorArm64
                | Self::WatchosArm64
                | Self::WatchosSimulatorArm64
        )
    }

    /// Returns the Kotlin compiler binary name for this target.
    pub fn compiler_name(&self) -> &'static str {
        match self {
            Self::Jvm | Self::Android => "kotlinc",
            Self::Js => "kotlinc-js",
            Self::WasmJs | Self::WasmWasi => "kotlinc",
            _ => "kotlinc-native",
        }
    }
}
