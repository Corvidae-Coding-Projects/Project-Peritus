//! Malformed proposals have no scope, receipt, filesystem or command effects.
use super::*;
use crate::workspace_delivery::scope::ScopedBaseline;

#[test]
fn bad_write_is_repairable_in_the_same_invocation_without_enrollment_or_effect() {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let scope =
        ScopedBaseline::new(root.path().to_owned(), state.path().join("files.jsonl"), vec![], 1);
    let mut tools = writable_tools(root.path()).with_in_place_scope(Some(scope.clone()));
    let _ = execute(&mut tools, "workspace_list", r#"{"path":"."}"#);
    let bad = execute(&mut tools, "workspace_write", r#"{"path":"Cargo.toml","contents":"typo"}"#);
    assert!(bad.is_error);
    assert!(wire(&bad).contains("No action was executed"));
    assert!(scope.paths().unwrap().is_empty());
    assert!(!root.path().join("Cargo.toml").exists());
    let good =
        execute(&mut tools, "workspace_write", r#"{"path":"Cargo.toml","content":"created"}"#);
    assert!(!good.is_error, "{}", wire(&good));
    assert_eq!(fs::read_to_string(root.path().join("Cargo.toml")).unwrap(), "created");
}

#[test]
fn absent_manifest_read_does_not_change_progress_or_block_greenfield_creation() {
    let root = tempfile::tempdir().unwrap();
    let state = tempfile::tempdir().unwrap();
    let scope =
        ScopedBaseline::new(root.path().to_owned(), state.path().join("files.jsonl"), vec![], 1);
    let before = scope.progress_checkpoint(root.path()).unwrap();
    let mut tools = writable_tools(root.path()).with_in_place_scope(Some(scope.clone()));
    let _ = execute(&mut tools, "workspace_list", r#"{"path":"."}"#);
    let read = execute(&mut tools, "workspace_read", r#"{"path":"Cargo.toml"}"#);
    assert!(read.is_error);
    assert!(wire(&read).contains("not_found"));
    assert!(wire(&read).contains("workspace_write"));
    assert_eq!(scope.progress_checkpoint(root.path()).unwrap(), before);
    let written =
        execute(&mut tools, "workspace_write", r#"{"path":"Cargo.toml","content":"created"}"#);
    assert!(!written.is_error, "{}", wire(&written));
    assert_ne!(scope.progress_checkpoint(root.path()).unwrap(), before);
}

#[test]
fn malformed_command_is_rejected_before_a_receipt_or_process_can_start() {
    let root = tempfile::tempdir().unwrap();
    let mut tools = writable_tools(root.path());
    let _ = execute(&mut tools, "workspace_list", r#"{"path":"."}"#);
    let bad = execute(
        &mut tools,
        "run_command",
        r#"{"program":"rustc","args":["--version"],"purpose":"verification","timeout_seconds":"600"}"#,
    );
    assert!(bad.is_error);
    assert!(wire(&bad).contains("timeout_seconds must be an integer"));
    assert!(tools.successful_commands().is_empty());
    assert!(fs::read_dir(root.path()).unwrap().next().is_none());
}
