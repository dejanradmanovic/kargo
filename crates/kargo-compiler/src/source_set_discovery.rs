//! Discover Kotlin source sets from the project layout on disk.
//!
//! Supports two layout conventions:
//! - **JVM-only**: `src/main/kotlin/`, `src/test/kotlin/`
//! - **KMP (multiplatform)**: `src/commonMain/kotlin/`, `src/jvmMain/kotlin/`, etc.

use std::path::{Path, PathBuf};

use kargo_core::manifest::Manifest;
use kargo_core::source_set::SourceSet;
use kargo_core::target::KotlinTarget;

/// Collected source sets for a project, split into main and test groups.
#[derive(Debug)]
pub struct DiscoveredSources {
    pub main_sources: Vec<SourceSet>,
    pub test_sources: Vec<SourceSet>,
}

/// Discover source sets based on the project manifest and directory structure.
///
/// When only a single JVM target is defined and no `commonMain` exists,
/// the simpler `src/main/kotlin` layout is used. Otherwise, the KMP
/// multiplatform layout (`src/commonMain/kotlin`, `src/<target>Main/kotlin`)
/// is assumed.
pub fn discover(project_root: &Path, manifest: &Manifest) -> DiscoveredSources {
    let src = project_root.join("src");
    let is_multiplatform = manifest.targets.len() > 1 || src.join("commonMain").is_dir();

    if is_multiplatform {
        discover_kmp(&src, manifest)
    } else {
        discover_single_target(&src)
    }
}

fn discover_single_target(src: &Path) -> DiscoveredSources {
    let main = SourceSet::new("main", src.to_path_buf());
    let test = SourceSet::new("test", src.to_path_buf()).with_depends_on("main");

    DiscoveredSources {
        main_sources: vec![main],
        test_sources: vec![test],
    }
}

fn discover_kmp(src: &Path, manifest: &Manifest) -> DiscoveredSources {
    let mut main_sources = Vec::new();
    let mut test_sources = Vec::new();

    let common_main = SourceSet::new("commonMain", src.to_path_buf());
    let common_test = SourceSet::new("commonTest", src.to_path_buf()).with_depends_on("commonMain");

    main_sources.push(common_main);
    test_sources.push(common_test);

    for key in manifest.targets.keys() {
        let Some(target) = KotlinTarget::parse(key) else {
            continue;
        };
        let ss_name = target.source_set_name();

        let target_main = SourceSet::new(format!("{ss_name}Main"), src.to_path_buf())
            .with_depends_on("commonMain");
        let target_test = SourceSet::new(format!("{ss_name}Test"), src.to_path_buf())
            .with_depends_on("commonTest")
            .with_depends_on(format!("{ss_name}Main"));

        main_sources.push(target_main);
        test_sources.push(target_test);
    }

    DiscoveredSources {
        main_sources,
        test_sources,
    }
}

/// Recursively collect all `.kt` files from the given directories.
pub fn collect_kotlin_files(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in dirs {
        if dir.is_dir() {
            collect_files_recursive(dir, &mut files);
        }
    }
    files.sort();
    files
}

/// Public recursive file collector used by other modules.
pub fn collect_files_recursive_pub(dir: &Path, out: &mut Vec<PathBuf>) {
    collect_files_recursive(dir, out);
}

fn collect_files_recursive(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_recursive(&path, out);
        } else if path
            .extension()
            .is_some_and(|ext| ext == "kt" || ext == "java")
        {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn minimal_manifest(targets: &[&str]) -> Manifest {
        let mut target_map = BTreeMap::new();
        for t in targets {
            target_map.insert(
                t.to_string(),
                kargo_core::target::TargetConfig {
                    java_target: None,
                    module_kind: None,
                    cinterop: None,
                    min_sdk: None,
                    target_sdk: None,
                    compile_sdk: None,
                },
            );
        }
        Manifest {
            package: kargo_core::manifest::PackageMetadata {
                name: "test".into(),
                group: None,
                version: "0.1.0".into(),
                kotlin: "2.3.0".into(),
                description: None,
                authors: vec![],
                license: None,
                repository: None,
                main_class: None,
                ksp_version: None,
            },
            targets: target_map,
            compose: None,
            dependencies: BTreeMap::new(),
            dev_dependencies: BTreeMap::new(),
            target: BTreeMap::new(),
            flavor: BTreeMap::new(),
            plugins: BTreeMap::new(),
            flavors: None,
            hooks: BTreeMap::new(),
            lint: None,
            format: None,
            profile: BTreeMap::new(),
            repositories: BTreeMap::new(),
            workspace: None,
            toolchain: None,
            catalog: None,
            test: None,
            signing: None,
            docker: None,
            ksp: BTreeMap::new(),
            ksp_options: BTreeMap::new(),
            kapt: BTreeMap::new(),
            kapt_options: BTreeMap::new(),
            build_config: BTreeMap::new(),
        }
    }

    #[test]
    fn single_target_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(src.join("main/kotlin")).unwrap();
        std::fs::create_dir_all(src.join("test/kotlin")).unwrap();

        let manifest = minimal_manifest(&["jvm"]);
        let result = discover(tmp.path(), &manifest);

        assert_eq!(result.main_sources.len(), 1);
        assert_eq!(result.main_sources[0].name, "main");
        assert_eq!(result.test_sources.len(), 1);
        assert_eq!(result.test_sources[0].name, "test");
    }

    #[test]
    fn kmp_layout() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(src.join("commonMain/kotlin")).unwrap();
        std::fs::create_dir_all(src.join("jvmMain/kotlin")).unwrap();

        let manifest = minimal_manifest(&["jvm", "js"]);
        let result = discover(tmp.path(), &manifest);

        assert!(result.main_sources.len() >= 3);
        let names: Vec<&str> = result
            .main_sources
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert!(names.contains(&"commonMain"));
        assert!(names.contains(&"jvmMain"));
        assert!(names.contains(&"jsMain"));
    }

    #[test]
    fn collect_kt_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("src/main/kotlin/com/example");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Main.kt"), "fun main() {}").unwrap();
        std::fs::write(dir.join("Helper.kt"), "class Helper").unwrap();
        std::fs::write(dir.join("readme.txt"), "not kotlin").unwrap();

        let files = collect_kotlin_files(&[tmp.path().join("src/main/kotlin")]);
        assert_eq!(files.len(), 2);
        assert!(files.iter().all(|f| f.extension().unwrap() == "kt"));
    }
}
