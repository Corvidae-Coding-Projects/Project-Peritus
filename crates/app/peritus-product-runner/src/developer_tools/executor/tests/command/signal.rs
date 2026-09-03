use super::*;

#[test]
fn active_commands_deliver_interrupt_signal_to_the_owned_process() {
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
            r#"{{"args":["--exact","developer_tools::executor::tests::command::command_timeout_fixture","--nocapture"],"cwd":".","interactive":true,"program":{program},"purpose":"verification","timeout_seconds":10}}"#
        ),
    );
    assert!(!start.is_error, "{}", wire(&start));
    let started: Value = serde_json::from_str(&wire(&start)).expect("start result");
    let handle = started["handle"].as_str().expect("command handle");

    let signalled = execute(
        &mut tools,
        "command_signal",
        &format!(r#"{{"handle":"{handle}","signal":"interrupt"}}"#),
    );
    let signalled: Value = serde_json::from_str(&wire(&signalled)).expect("signal result");
    let terminal = if signalled["state"] == "completed" {
        signalled
    } else {
        poll_terminal(&mut tools, handle)
    };

    assert_eq!(terminal["state"], "completed", "{terminal}");
    assert_eq!(terminal["disposition"], "signalled", "{terminal}");
    assert_eq!(terminal["timed_out"], false, "{terminal}");
}
