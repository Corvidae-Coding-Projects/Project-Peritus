use super::*;

#[test]
fn read_only_tools_reject_undeclared_mutation_and_process_calls() {
    let workspace = tempfile::tempdir().expect("workspace");
    fs::write(workspace.path().join("README.md"), "before\n").expect("existing file");
    let mut tools = WorkspaceDeveloperTools::read_only(workspace.path().to_owned());

    let listed = execute(&mut tools, "workspace_list", r#"{"depth":2,"path":""}"#);
    let read = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":20,"path":"README.md","start_line":1}"#,
    );
    assert!(!listed.is_error);
    assert!(!read.is_error);

    for (name, arguments) in [
        ("workspace_write", r#"{"content":"after\n","path":"README.md"}"#),
        ("workspace_patch", r#"{"new":"after","old":"before","path":"README.md"}"#),
        ("workspace_remove", r#"{"path":"README.md"}"#),
        ("run_command", r#"{"args":["status"],"program":"git"}"#),
        ("command_start", r#"{"args":["status"],"program":"git","purpose":"verification"}"#),
        ("command_poll", r#"{"handle":"active"}"#),
        ("command_stdin", r#"{"handle":"active","text":"input"}"#),
        ("command_resize", r#"{"columns":80,"handle":"active","rows":24}"#),
        ("command_signal", r#"{"handle":"active","signal":"interrupt"}"#),
        ("command_cancel", r#"{"handle":"active"}"#),
        ("command_recover", r#"{"handle":"active"}"#),
    ] {
        let refused = execute(&mut tools, name, arguments);
        assert!(refused.is_error, "{name} must be refused");
        assert!(wire(&refused).contains("read-only workspace access"));
    }
    assert_eq!(
        fs::read_to_string(workspace.path().join("README.md")).expect("unchanged file"),
        "before\n",
    );
}
