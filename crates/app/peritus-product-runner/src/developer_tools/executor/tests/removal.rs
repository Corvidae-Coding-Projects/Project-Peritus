//! Exact removals preserve observed ownership and external evidence.

use super::*;

#[test]
fn exact_remove_preserves_late_external_evidence_and_blocks_shell_deletion() {
    let workspace = tempfile::tempdir().expect("workspace");
    fs::write(workspace.path().join("baseline.txt"), "baseline\n").expect("baseline");
    let mut tools = writable_tools(workspace.path());
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":2,"path":""}"#);

    fs::write(workspace.path().join("api_access.log"), "/projects\n").expect("external log");
    for path in ["baseline.txt", "api_access.log"] {
        let _ = execute(
            &mut tools,
            "workspace_read",
            &format!(r#"{{"end_line":20,"path":"{path}","start_line":1}}"#),
        );
    }

    let refused = execute(&mut tools, "workspace_remove", r#"{"path":"api_access.log"}"#);
    assert!(refused.is_error);
    assert!(wire(&refused).contains("externally produced evidence"));
    assert!(workspace.path().join("api_access.log").is_file());

    let removed = execute(&mut tools, "workspace_remove", r#"{"path":"baseline.txt"}"#);
    assert!(!removed.is_error);
    assert!(!workspace.path().join("baseline.txt").exists());

    let shell_delete = execute(
        &mut tools,
        "run_command",
        r#"{"args":["api_access.log"],"cwd":".","program":"rm","purpose":"external_effect"}"#,
    );
    assert!(shell_delete.is_error);
    assert!(wire(&shell_delete).contains("workspace_remove"));
    assert!(workspace.path().join("api_access.log").is_file());

    let ownership = tools.ownership().clone();
    let mut fixer = WorkspaceDeveloperTools::with_ownership(
        workspace.path().to_owned(),
        ownership,
        receipt_path(workspace.path()),
        "fixer-test".to_owned(),
        Duration::from_secs(30),
        test_command_runtime(workspace.path()),
    );
    let _ = execute(&mut fixer, "workspace_list", r#"{"depth":1,"path":""}"#);
    let _ = execute(
        &mut fixer,
        "workspace_read",
        r#"{"end_line":20,"path":"api_access.log","start_line":1}"#,
    );
    let cross_invocation = execute(&mut fixer, "workspace_remove", r#"{"path":"api_access.log"}"#);
    assert!(cross_invocation.is_error);
    assert!(wire(&cross_invocation).contains("externally produced evidence"));
    assert!(workspace.path().join("api_access.log").is_file());
}

#[test]
fn workspace_remove_deletes_only_an_observed_empty_directory() {
    let workspace = tempfile::tempdir().expect("workspace");
    fs::write(workspace.path().join("README.md"), "grounding\n").expect("grounding file");
    fs::create_dir_all(workspace.path().join("out/tmp")).expect("empty directory");
    fs::create_dir_all(workspace.path().join("out/nonempty")).expect("nonempty directory");
    fs::write(workspace.path().join("out/nonempty/evidence.log"), "retain\n")
        .expect("retained file");
    let mut tools = writable_tools(workspace.path());
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":4,"path":""}"#);
    let _ = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":20,"path":"README.md","start_line":1}"#,
    );

    let removed = execute(&mut tools, "workspace_remove", r#"{"path":"out/tmp"}"#);
    assert!(!removed.is_error);
    assert!(wire(&removed).contains(r#""kind":"directory""#));
    assert!(!workspace.path().join("out/tmp").exists());

    let nonempty = execute(&mut tools, "workspace_remove", r#"{"path":"out/nonempty"}"#);
    assert!(nonempty.is_error);
    assert!(wire(&nonempty).contains("only removes an empty directory"));
    assert!(workspace.path().join("out/nonempty/evidence.log").is_file());

    let root = execute(&mut tools, "workspace_remove", r#"{"path":"."}"#);
    assert!(root.is_error);
    assert!(wire(&root).contains("cannot remove the workspace root"));
}
