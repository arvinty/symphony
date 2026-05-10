use std::path::PathBuf;
use symphony_core::config::HooksConfig;
use symphony_core::model::sanitize_workspace_key;
use symphony_core::workspace::{verify_inside_root, WorkspaceManager};

fn temp_dir() -> PathBuf {
    let p = std::env::temp_dir().join(format!("symphony_ws_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn sanitization_strips_unsafe_chars() {
    assert_eq!(sanitize_workspace_key("ABC-123"), "ABC-123");
    assert_eq!(sanitize_workspace_key("a/b\\c d"), "a_b_c_d");
    assert_eq!(sanitize_workspace_key("../etc"), ".._etc");
}

#[test]
fn verify_inside_root_accepts_child() {
    let root = temp_dir();
    let child = root.join("DEMO-1");
    verify_inside_root(&root, &child).unwrap();
}

#[test]
fn verify_inside_root_rejects_escape() {
    let root = temp_dir();
    let outside = root.parent().unwrap().join("escape_DO_NOT");
    assert!(verify_inside_root(&root, &outside).is_err());
}

#[tokio::test]
async fn ensure_creates_workspace_and_marks_created_now() {
    let root = temp_dir();
    let mgr = WorkspaceManager::new(root.clone(), HooksConfig::default());
    let ws = mgr.ensure("DEMO-1").await.unwrap();
    assert!(ws.created_now);
    assert!(ws.path.exists());
    assert_eq!(ws.workspace_key, "DEMO-1");

    let again = mgr.ensure("DEMO-1").await.unwrap();
    assert!(!again.created_now);
}

#[tokio::test]
async fn ensure_sanitizes_identifier_for_path() {
    let root = temp_dir();
    let mgr = WorkspaceManager::new(root.clone(), HooksConfig::default());
    let ws = mgr.ensure("a/b c").await.unwrap();
    assert_eq!(ws.workspace_key, "a_b_c");
    assert!(ws.path.starts_with(&root));
}
