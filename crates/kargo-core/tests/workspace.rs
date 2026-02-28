use kargo_core::manifest::Manifest;
use kargo_core::package::Package;
use kargo_core::workspace::Workspace;
use std::path::PathBuf;

fn dummy_package(root: PathBuf) -> Package {
    let manifest = Manifest::parse_toml(
        r#"
[package]
name = "test"
version = "0.1.0"
kotlin = "2.3.0"
"#,
    )
    .unwrap();
    Package {
        manifest,
        manifest_path: root.join("Kargo.toml"),
        root_dir: root,
    }
}

#[test]
fn test_workspace_single_member_same_root_not_virtual() {
    let root = PathBuf::from("/project");
    let ws = Workspace {
        root_dir: root.clone(),
        members: vec![dummy_package(root)],
    };
    assert!(!ws.is_virtual());
}

#[test]
fn test_workspace_single_member_different_root_is_virtual() {
    let ws = Workspace {
        root_dir: PathBuf::from("/workspace"),
        members: vec![dummy_package(PathBuf::from("/workspace/app"))],
    };
    assert!(ws.is_virtual());
}

#[test]
fn test_workspace_multiple_members_is_virtual() {
    let root = PathBuf::from("/workspace");
    let ws = Workspace {
        root_dir: root.clone(),
        members: vec![
            dummy_package(PathBuf::from("/workspace/app")),
            dummy_package(PathBuf::from("/workspace/shared")),
        ],
    };
    assert!(ws.is_virtual());
}

#[test]
fn test_workspace_no_members_is_virtual() {
    let ws = Workspace {
        root_dir: PathBuf::from("/project"),
        members: vec![],
    };
    assert!(ws.is_virtual());
}
