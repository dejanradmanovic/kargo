use kargo_core::source_set::SourceSet;
use std::path::PathBuf;

#[test]
fn new_sets_correct_kotlin_and_resource_dirs() {
    let base = PathBuf::from("/project/src");
    let set = SourceSet::new("commonMain", base.clone());

    assert_eq!(
        set.kotlin_dirs,
        vec![PathBuf::from("/project/src/commonMain/kotlin")]
    );
    assert_eq!(
        set.resource_dirs,
        vec![PathBuf::from("/project/src/commonMain/resources")]
    );
}

#[test]
fn new_name_stored_correctly() {
    let base = PathBuf::from("/base");
    let set = SourceSet::new("jvmMain", base);
    assert_eq!(set.name, "jvmMain");
}

#[test]
fn with_depends_on_adds_parent() {
    let base = PathBuf::from("/base");
    let set = SourceSet::new("freeMain", base).with_depends_on("commonMain");
    assert!(set.depends_on.contains("commonMain"));
}

#[test]
fn with_depends_on_chained() {
    let base = PathBuf::from("/base");
    let set = SourceSet::new("freeRelease", base)
        .with_depends_on("commonMain")
        .with_depends_on("freeMain");
    assert!(set.depends_on.contains("commonMain"));
    assert!(set.depends_on.contains("freeMain"));
    assert_eq!(set.depends_on.len(), 2);
}

#[test]
fn exists_returns_false_for_nonexistent_dirs() {
    let base = PathBuf::from("/nonexistent/nowhere");
    let set = SourceSet::new("main", base);
    assert!(!set.exists());
}
