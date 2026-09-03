use super::*;

#[test]
fn rejected_pipe_controls_preserve_the_active_command_handle() {
    let workspace = tempfile::tempdir().expect("workspace");
    let mut tools = writable_tools(workspace.path());
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    fs::write(workspace.path().join(".peritus-timeout-fixture"), "run").expect("fixture marker");
    let executable = std::env::current_exe().expect("current test executable");
    let program = serde_json::to_string(&executable).expect("program path");
    let start = execute(
        &mut tools,
        "command_start",
        &format!(
            r#"{{"args":["--exact","developer_tools::executor::tests::command::command_timeout_fixture","--nocapture"],"cwd":".","interactive":false,"program":{program},"purpose":"verification","timeout_seconds":10}}"#
        ),
    );
    assert!(!start.is_error, "{}", wire(&start));
    let started: Value = serde_json::from_str(&wire(&start)).expect("start result");
    let handle = started["handle"].as_str().expect("command handle");

    let resized = execute(
        &mut tools,
        "command_resize",
        &format!(r#"{{"columns":100,"handle":"{handle}","rows":30}}"#),
    );
    assert!(resized.is_error, "{}", wire(&resized));
    assert!(wire(&resized).contains("terminal resize was not authorized"));

    let input = execute(
        &mut tools,
        "command_stdin",
        &format!(r#"{{"handle":"{handle}","text":"not accepted\n"}}"#),
    );
    assert!(input.is_error, "{}", wire(&input));
    assert!(wire(&input).contains("stdin is disabled for this process"));

    let recovered = execute(&mut tools, "command_recover", &format!(r#"{{"handle":"{handle}"}}"#));
    assert!(!recovered.is_error, "{}", wire(&recovered));
    let cancelled = execute(&mut tools, "command_cancel", &format!(r#"{{"handle":"{handle}"}}"#));
    assert!(!cancelled.is_error || wire(&cancelled).contains("cancelled"));
    assert_eq!(poll_terminal(&mut tools, handle)["status"], "cancelled");
}
