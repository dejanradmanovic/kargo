//! Source set hierarchy inference for KMP: appleMain, nativeMain, etc.
//!
//! Defines the standard Kotlin/Native target hierarchy so that intermediate
//! source sets (e.g. `nativeMain`, `appleMain`, `iosMain`) are automatically
//! created and wired when a project declares leaf targets.
//!
//! ```text
//! commonMain
//!   |-- jvmMain
//!   |-- jsMain
//!   |-- wasmJsMain
//!   |-- wasmWasiMain
//!   |-- nativeMain
//!         |-- appleMain
//!         |     |-- iosMain
//!         |     |     |-- iosArm64Main, iosSimulatorArm64Main, iosX64Main
//!         |     |-- macosMain
//!         |     |     |-- macosArm64Main, macosX64Main
//!         |     |-- tvosMain
//!         |     |     |-- tvosArm64Main, tvosSimulatorArm64Main
//!         |     |-- watchosMain
//!         |           |-- watchosArm64Main, watchosSimulatorArm64Main
//!         |-- linuxMain
//!         |     |-- linuxX64Main, linuxArm64Main
//!         |-- mingwMain
//!         |     |-- mingwX64Main
//!         |-- androidNativeMain
//!               |-- androidNativeArm64Main, androidNativeX64Main
//! ```

use std::collections::HashMap;

/// The standard KMP source set hierarchy.
///
/// Maps each intermediate or leaf source set name to its parent.
/// `"commonMain"` is the root and has no parent.
pub struct SourceSetHierarchy {
    /// source_set_name -> parent_source_set_name
    parent: HashMap<&'static str, &'static str>,
}

impl SourceSetHierarchy {
    /// Build the standard Kotlin hierarchy.
    pub fn standard() -> Self {
        let mut parent = HashMap::new();

        // Direct children of commonMain
        parent.insert("jvm", "common");
        parent.insert("android", "common");
        parent.insert("js", "common");
        parent.insert("wasmJs", "common");
        parent.insert("wasmWasi", "common");
        parent.insert("native", "common");

        // nativeMain subtree
        parent.insert("apple", "native");
        parent.insert("linux", "native");
        parent.insert("mingw", "native");
        parent.insert("androidNative", "native");

        // appleMain subtree
        parent.insert("ios", "apple");
        parent.insert("macos", "apple");
        parent.insert("tvos", "apple");
        parent.insert("watchos", "apple");

        // Leaf targets under ios
        parent.insert("iosArm64", "ios");
        parent.insert("iosSimulatorArm64", "ios");
        parent.insert("iosX64", "ios");

        // Leaf targets under macos
        parent.insert("macosArm64", "macos");
        parent.insert("macosX64", "macos");

        // Leaf targets under tvos
        parent.insert("tvosArm64", "tvos");
        parent.insert("tvosSimulatorArm64", "tvos");

        // Leaf targets under watchos
        parent.insert("watchosArm64", "watchos");
        parent.insert("watchosSimulatorArm64", "watchos");

        // Leaf targets under linux
        parent.insert("linuxX64", "linux");
        parent.insert("linuxArm64", "linux");

        // Leaf targets under mingw
        parent.insert("mingwX64", "mingw");

        // Leaf targets under androidNative
        parent.insert("androidNativeArm64", "androidNative");
        parent.insert("androidNativeX64", "androidNative");

        Self { parent }
    }

    /// Walk from a leaf source set up to `common`, returning all
    /// intermediate source set names in order from leaf to root.
    ///
    /// For example, `ancestors_of("iosArm64")` returns
    /// `["ios", "apple", "native", "common"]`.
    pub fn ancestors_of(&self, source_set: &str) -> Vec<&'static str> {
        let mut ancestors = Vec::new();
        let mut current = source_set;
        while let Some(&p) = self.parent.get(current) {
            ancestors.push(p);
            current = p;
        }
        ancestors
    }

    /// Collect all unique intermediate source sets needed for a set of
    /// leaf targets. This determines which `<name>Main` / `<name>Test`
    /// directories should be created.
    ///
    /// For example, given `["iosArm64", "macosArm64", "jvm"]`, returns
    /// `{"ios", "macos", "apple", "native", "common", "jvm"}` (unordered).
    pub fn intermediates_for(&self, source_set_names: &[&str]) -> Vec<&'static str> {
        let mut result = std::collections::HashSet::new();
        for name in source_set_names {
            // The leaf itself (already added by caller as a target source set)
            for ancestor in self.ancestors_of(name) {
                result.insert(ancestor);
            }
        }
        let mut sorted: Vec<&str> = result.into_iter().collect();
        sorted.sort();
        sorted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ancestors_of_leaf_target() {
        let h = SourceSetHierarchy::standard();
        let ancestors = h.ancestors_of("iosArm64");
        assert_eq!(ancestors, vec!["ios", "apple", "native", "common"]);
    }

    #[test]
    fn ancestors_of_jvm() {
        let h = SourceSetHierarchy::standard();
        let ancestors = h.ancestors_of("jvm");
        assert_eq!(ancestors, vec!["common"]);
    }

    #[test]
    fn ancestors_of_intermediate() {
        let h = SourceSetHierarchy::standard();
        let ancestors = h.ancestors_of("apple");
        assert_eq!(ancestors, vec!["native", "common"]);
    }

    #[test]
    fn ancestors_of_unknown() {
        let h = SourceSetHierarchy::standard();
        let ancestors = h.ancestors_of("doesNotExist");
        assert!(ancestors.is_empty());
    }

    #[test]
    fn intermediates_for_mixed_targets() {
        let h = SourceSetHierarchy::standard();
        let intermediates = h.intermediates_for(&["iosArm64", "jvm"]);
        assert!(intermediates.contains(&"common"));
        assert!(intermediates.contains(&"native"));
        assert!(intermediates.contains(&"apple"));
        assert!(intermediates.contains(&"ios"));
    }

    #[test]
    fn intermediates_deduplicates() {
        let h = SourceSetHierarchy::standard();
        let intermediates = h.intermediates_for(&["iosArm64", "iosX64"]);
        let ios_count = intermediates.iter().filter(|&&s| s == "ios").count();
        assert_eq!(ios_count, 1);
    }
}
