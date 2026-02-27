use kargo_core::template::{interpolate, ProjectTemplate, TemplateContext, TemplateRegistry};
use tempfile::TempDir;

#[test]
fn test_interpolate_replaces_known_vars() {
    let ctx = TemplateContext::new("my-app", "2.3.0");
    let result = interpolate("name={{project_name}} kotlin={{kotlin_version}}", &ctx);
    assert_eq!(result, "name=my-app kotlin=2.3.0");
}

#[test]
fn test_interpolate_unknown_var_left_intact() {
    let ctx = TemplateContext::new("app", "2.3.0");
    let result = interpolate("{{unknown_var}}", &ctx);
    assert_eq!(result, "{{unknown_var}}");
}

#[test]
fn test_interpolate_no_placeholders() {
    let ctx = TemplateContext::new("app", "2.3.0");
    let result = interpolate("no placeholders here", &ctx);
    assert_eq!(result, "no placeholders here");
}

#[test]
fn test_interpolate_custom_variable() {
    let mut ctx = TemplateContext::new("app", "2.3.0");
    ctx.set("custom_key", "custom_value");
    let result = interpolate("{{custom_key}}", &ctx);
    assert_eq!(result, "custom_value");
}

#[test]
fn test_template_from_str_valid() {
    let toml = r##"
[template]
name = "test"
description = "Test template"

[manifest]
content = "[package]\nname = \"test\""

[[directories]]
path = "src"

[[files]]
path = "hello.txt"
content = "Hello {{project_name}}"
"##;
    let tmpl = ProjectTemplate::from_str(toml).unwrap();
    assert_eq!(tmpl.template.name, "test");
    assert_eq!(tmpl.directories.len(), 1);
    assert_eq!(tmpl.files.len(), 1);
}

#[test]
fn test_template_from_str_invalid() {
    let result = ProjectTemplate::from_str("not valid toml {{");
    assert!(result.is_err());
}

#[test]
fn test_template_render_creates_all_files() {
    let toml = r##"
[template]
name = "test"
description = "Test template"

[manifest]
content = "[package]\nname = \"{{project_name}}\"\nversion = \"0.1.0\"\nkotlin = \"{{kotlin_version}}\""

[[directories]]
path = "src/main"

[[directories]]
path = "src/test"

[[files]]
path = "README.md"
content = "# {{project_name}}"

[[files]]
path = "src/main/Hello.kt"
content = "fun main() = println(\"{{project_name}}\")"
"##;

    let tmpl = ProjectTemplate::from_str(toml).unwrap();
    let tmp = TempDir::new().unwrap();
    let root = tmp.path().join("my-project");
    std::fs::create_dir(&root).unwrap();

    let ctx = TemplateContext::new("my-project", "2.3.0");
    tmpl.render(&root, &ctx).unwrap();

    assert!(root.join("Kargo.toml").is_file());
    assert!(root.join("Kargo.lock").is_file());
    assert!(root.join(".gitignore").is_file());
    assert!(root.join(".kargo.env").is_file());
    assert!(root.join("src/main").is_dir());
    assert!(root.join("src/test").is_dir());
    assert!(root.join("README.md").is_file());
    assert!(root.join("src/main/Hello.kt").is_file());

    let manifest = std::fs::read_to_string(root.join("Kargo.toml")).unwrap();
    assert!(manifest.contains(r#"name = "my-project""#));

    let readme = std::fs::read_to_string(root.join("README.md")).unwrap();
    assert_eq!(readme, "# my-project");

    let hello = std::fs::read_to_string(root.join("src/main/Hello.kt")).unwrap();
    assert!(hello.contains("my-project"));
}

#[test]
fn test_render_core_only_skips_existing_and_omits_src() {
    let toml = r##"
[template]
name = "test"
description = "Test"

[manifest]
content = "overwritten"

[[directories]]
path = "src/main/kotlin"

[[files]]
path = "src/main/kotlin/Main.kt"
content = "fun main() {}"
"##;
    let tmpl = ProjectTemplate::from_str(toml).unwrap();
    let tmp = TempDir::new().unwrap();

    std::fs::write(tmp.path().join("Kargo.toml"), "original manifest").unwrap();

    let ctx = TemplateContext::new("app", "2.3.0");
    tmpl.render_core_only(tmp.path(), &ctx).unwrap();

    let manifest = std::fs::read_to_string(tmp.path().join("Kargo.toml")).unwrap();
    assert_eq!(manifest, "original manifest", "existing Kargo.toml should not be overwritten");

    assert!(tmp.path().join("Kargo.lock").is_file(), "Kargo.lock should be created");
    assert!(tmp.path().join(".gitignore").is_file(), ".gitignore should be created");
    assert!(tmp.path().join(".kargo.env").is_file(), ".kargo.env should be created");

    assert!(!tmp.path().join("src/main/kotlin").exists(), "render_core_only must not create src dirs");
    assert!(!tmp.path().join("src/main/kotlin/Main.kt").exists(), "render_core_only must not create src files");
}

#[test]
fn test_registry_loads_all_builtin_templates() {
    let registry = TemplateRegistry::new().unwrap();
    let names = registry.names();
    assert!(names.contains(&"jvm"));
    assert!(names.contains(&"lib"));
    assert!(names.contains(&"kmp"));
    assert!(names.contains(&"cmp"));
    assert!(names.contains(&"android"));
    assert_eq!(names.len(), 5);
}

#[test]
fn test_registry_get_returns_correct_template() {
    let registry = TemplateRegistry::new().unwrap();
    let jvm = registry.get("jvm").unwrap();
    assert_eq!(jvm.template.name, "jvm");

    let cmp = registry.get("cmp").unwrap();
    assert_eq!(cmp.template.name, "cmp");
}

#[test]
fn test_registry_get_unknown_returns_none() {
    let registry = TemplateRegistry::new().unwrap();
    assert!(registry.get("nonexistent").is_none());
}

#[test]
fn test_registry_list_returns_descriptions() {
    let registry = TemplateRegistry::new().unwrap();
    let list = registry.list();
    assert_eq!(list.len(), 5);
    for (name, desc) in &list {
        assert!(!name.is_empty());
        assert!(!desc.is_empty());
    }
}

#[test]
fn test_cmp_template_differs_from_kmp() {
    let registry = TemplateRegistry::new().unwrap();
    let kmp = registry.get("kmp").unwrap();
    let cmp = registry.get("cmp").unwrap();

    assert!(
        cmp.manifest.content.contains("[compose]"),
        "cmp must enable compose"
    );
    assert!(
        !kmp.manifest.content.contains("[compose]"),
        "kmp must not enable compose"
    );

    assert!(
        cmp.manifest.content.contains("android"),
        "cmp must include android target"
    );
    assert!(
        !kmp.manifest.content.contains("android"),
        "kmp must not include android target"
    );

    let cmp_dirs: Vec<&str> = cmp.directories.iter().map(|d| d.path.as_str()).collect();
    assert!(cmp_dirs.contains(&"src/androidMain/kotlin"));
    assert!(cmp_dirs.contains(&"src/desktopMain/kotlin"));

    let kmp_dirs: Vec<&str> = kmp.directories.iter().map(|d| d.path.as_str()).collect();
    assert!(!kmp_dirs.contains(&"src/androidMain/kotlin"));
    assert!(!kmp_dirs.contains(&"src/desktopMain/kotlin"));
}
