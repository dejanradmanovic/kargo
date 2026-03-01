pub mod ops_add;
pub mod ops_audit;
pub mod ops_build;
pub mod ops_cache;
pub mod ops_check;
pub mod ops_clean;
pub mod ops_fetch;
pub mod ops_init;
pub mod ops_lock;
pub mod ops_new;
pub mod ops_outdated;
pub mod ops_remove;
pub mod ops_run;
pub mod ops_self;
pub mod ops_self_update;
pub mod ops_setup;
pub mod ops_test;
pub mod ops_toolchain;
pub mod ops_tree;
pub mod ops_update;

use std::path::Path;

/// Build a classpath string from a list of JARs, appending the Kotlin stdlib.
pub fn classpath_string_with_stdlib(jars: &[std::path::PathBuf], kotlin_home: &Path) -> String {
    let kotlin_lib = kotlin_home.join("lib");
    let mut all: Vec<std::path::PathBuf> = jars.to_vec();
    for name in &[
        "kotlin-stdlib.jar",
        "annotations-13.0.jar",
        "kotlin-annotations-jvm.jar",
    ] {
        let jar = kotlin_lib.join(name);
        if jar.is_file() && !all.iter().any(|p| p.ends_with(name)) {
            all.push(jar);
        }
    }
    kargo_compiler::classpath::to_classpath_string(&all)
}
