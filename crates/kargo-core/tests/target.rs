use kargo_core::target::KotlinTarget;

#[test]
fn from_str_jvm() {
    assert_eq!(KotlinTarget::from_str("jvm"), Some(KotlinTarget::Jvm));
}

#[test]
fn from_str_ios_arm64() {
    assert_eq!(KotlinTarget::from_str("ios-arm64"), Some(KotlinTarget::IosArm64));
}

#[test]
fn from_str_ios_arm64_camel_case() {
    assert_eq!(KotlinTarget::from_str("iosArm64"), Some(KotlinTarget::IosArm64));
}

#[test]
fn from_str_invalid() {
    assert_eq!(KotlinTarget::from_str("invalid"), None);
}

#[test]
fn from_str_android() {
    assert_eq!(KotlinTarget::from_str("android"), Some(KotlinTarget::Android));
}

#[test]
fn is_native_false_for_jvm_android_js_wasm() {
    assert!(!KotlinTarget::Jvm.is_native());
    assert!(!KotlinTarget::Android.is_native());
    assert!(!KotlinTarget::Js.is_native());
    assert!(!KotlinTarget::WasmJs.is_native());
    assert!(!KotlinTarget::WasmWasi.is_native());
}

#[test]
fn is_native_true_for_ios_linux() {
    assert!(KotlinTarget::IosArm64.is_native());
    assert!(KotlinTarget::LinuxX64.is_native());
}

#[test]
fn is_android_true_only_for_android() {
    assert!(KotlinTarget::Android.is_android());
    assert!(!KotlinTarget::Jvm.is_android());
    assert!(!KotlinTarget::AndroidNativeArm64.is_android());
}

#[test]
fn is_apple_true_for_all_apple_targets() {
    assert!(KotlinTarget::IosArm64.is_apple());
    assert!(KotlinTarget::IosSimulatorArm64.is_apple());
    assert!(KotlinTarget::IosX64.is_apple());
    assert!(KotlinTarget::MacosArm64.is_apple());
    assert!(KotlinTarget::MacosX64.is_apple());
    assert!(KotlinTarget::TvosArm64.is_apple());
    assert!(KotlinTarget::TvosSimulatorArm64.is_apple());
    assert!(KotlinTarget::WatchosArm64.is_apple());
    assert!(KotlinTarget::WatchosSimulatorArm64.is_apple());
}

#[test]
fn is_apple_false_for_non_apple() {
    assert!(!KotlinTarget::Jvm.is_apple());
    assert!(!KotlinTarget::Js.is_apple());
    assert!(!KotlinTarget::LinuxX64.is_apple());
    assert!(!KotlinTarget::MingwX64.is_apple());
}

#[test]
fn compiler_name_jvm() {
    assert_eq!(KotlinTarget::Jvm.compiler_name(), "kotlinc");
}

#[test]
fn compiler_name_js() {
    assert_eq!(KotlinTarget::Js.compiler_name(), "kotlinc-js");
}

#[test]
fn compiler_name_android() {
    assert_eq!(KotlinTarget::Android.compiler_name(), "kotlinc");
}

#[test]
fn compiler_name_native() {
    assert_eq!(KotlinTarget::IosArm64.compiler_name(), "kotlinc-native");
    assert_eq!(KotlinTarget::LinuxX64.compiler_name(), "kotlinc-native");
}
