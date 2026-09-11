//! Malformed proposals have no scope, receipt, filesystem or command effects.
use super::*;
use crate::workspace_delivery::scope::ScopedBaseline;
use crate::{
    ConversationView,
    control::{HostPermissions, PermissionCapability},
};
use std::sync::{Arc, Mutex};

struct MutablePermissionView {
    permissions: Mutex<HostPermissions>,
}

impl MutablePermissionView {
    fn new(permissions: HostPermissions) -> Self {
        Self { permissions: Mutex::new(permissions) }
    }

    fn set(&self, permissions: HostPermissions) {
        *self.permissions.lock().expect("permission view") = permissions;
    }
}

impl ConversationView for MutablePermissionView {
    fn revision(&self) -> u64 {
        0
    }

    fn render(&self) -> String {
        String::new()
    }

    fn effective_permissions(&self) -> HostPermissions {
        *self.permissions.lock().expect("permission view")
    }
}

#[test]
fn live_permission_narrowing_precedes_reads_scope_receipts_and_process_effects() {
    let root = tempfile::tempdir().expect("workspace");
    let state = tempfile::tempdir().expect("state");
    fs::write(root.path().join("secret.txt"), "TOP_SECRET_BYTES\n").expect("secret fixture");
    let scope =
        ScopedBaseline::new(root.path().to_owned(), state.path().join("files.jsonl"), vec![], 1);
    let view = Arc::new(MutablePermissionView::new(HostPermissions::all()));
    let mut tools = writable_tools(root.path())
        .with_in_place_scope(Some(scope.clone()))
        .with_protection_view(view.clone());

    let admitted = execute(
        &mut tools,
        "workspace_read",
        r#"{"path":"secret.txt","start_line":1,"end_line":5}"#,
    );
    assert!(!admitted.is_error, "{}", wire(&admitted));
    assert!(wire(&admitted).contains("TOP_SECRET_BYTES"));

    view.set(HostPermissions::all().without(PermissionCapability::Read));
    let denied_read = execute(
        &mut tools,
        "workspace_read",
        r#"{"path":"secret.txt","start_line":1,"end_line":5}"#,
    );
    assert!(denied_read.is_error);
    assert!(!wire(&denied_read).contains("TOP_SECRET_BYTES"));

    let enrolled_before_denial = scope.paths().expect("scope paths before denial");
    view.set(HostPermissions::all().without(PermissionCapability::Write));
    let denied_write = execute(
        &mut tools,
        "workspace_write",
        r#"{"path":"created.txt","content":"must not exist\n"}"#,
    );
    assert!(denied_write.is_error);
    assert!(!root.path().join("created.txt").exists());
    assert_eq!(scope.paths().expect("scope paths after denial"), enrolled_before_denial);

    view.set(HostPermissions::all().without(PermissionCapability::Process));
    let denied_process = execute(
        &mut tools,
        "run_command",
        r#"{"program":"sh","args":["-c","printf started > process-marker.txt"],"cwd":".","purpose":"verification"}"#,
    );
    assert!(denied_process.is_error);
    assert!(!root.path().join("process-marker.txt").exists());
    assert!(tools.successful_commands().is_empty());
    assert!(!receipt_path(root.path()).exists());
}

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
