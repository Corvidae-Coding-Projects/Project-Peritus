use super::*;
use std::{io::Write as _, time::Instant};

#[test]
fn structured_commands_time_out_without_freezing_the_agent() {
    let workspace = tempfile::tempdir().expect("workspace");
    let mut tools = writable_tools(workspace.path());
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    fs::write(workspace.path().join(".peritus-timeout-fixture"), "run").expect("fixture marker");
    let executable = std::env::current_exe().expect("current test executable");
    let program = serde_json::to_string(&executable).expect("program path");
    let arguments = format!(
        r#"{{"args":["--exact","developer_tools::executor::tests::command::command_timeout_fixture","--nocapture"],"cwd":".","program":{program},"timeout_seconds":1}}"#
    );

    let started = Instant::now();
    let command = execute(&mut tools, "run_command", &arguments);

    assert!(command.is_error);
    assert!(started.elapsed() < Duration::from_secs(10));
    let result: Value = serde_json::from_str(&wire(&command)).expect("command result JSON");
    assert_eq!(result["success"].as_bool(), Some(false));
    assert_eq!(result["timed_out"].as_bool(), Some(true));
    assert_eq!(result["requested_timeout_seconds"].as_u64(), Some(1));
    assert_eq!(result["timeout_seconds"].as_u64(), Some(1));
    assert_eq!(result["deadline_limited"].as_bool(), Some(false));
    assert!(
        result["recovery_hint"]
            .as_str()
            .is_some_and(|value| value.contains("another bulk-transfer wrapper"))
    );
}

#[test]
fn structured_commands_shrink_to_preserve_the_product_completion_reserve() {
    let workspace = tempfile::tempdir().expect("workspace");
    let mut tools = writable_tools_with_horizon(workspace.path(), Duration::from_secs(3));
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    fs::write(workspace.path().join(".peritus-timeout-fixture"), "run").expect("fixture marker");
    let executable = std::env::current_exe().expect("current test executable");
    let program = serde_json::to_string(&executable).expect("program path");
    let arguments = format!(
        r#"{{"args":["--exact","developer_tools::executor::tests::command::command_timeout_fixture","--nocapture"],"cwd":".","program":{program},"timeout_seconds":10}}"#
    );

    let command = execute(&mut tools, "run_command", &arguments);

    assert!(command.is_error);
    let result: Value = serde_json::from_str(&wire(&command)).expect("command result JSON");
    assert_eq!(result["requested_timeout_seconds"].as_u64(), Some(10));
    let actual = result["timeout_seconds"].as_u64().expect("actual timeout");
    assert!(actual <= 1);
    assert_eq!(result["deadline_limited"].as_bool(), Some(true));
    assert_eq!(result["completion_reserve_seconds"].as_u64(), Some(1));
    let recovery = result["recovery_hint"].as_str().expect("deadline recovery");
    if actual == 0 {
        assert_eq!(result["timed_out"].as_bool(), Some(false));
        assert!(recovery.contains("was not started"));
    } else {
        assert_eq!(result["timed_out"].as_bool(), Some(true));
        assert!(recovery.contains("live product-budget allowance"));
    }
}

#[test]
fn missing_executable_returns_scoped_prerequisite_recovery_guidance() {
    let workspace = tempfile::tempdir().expect("workspace");
    let mut tools = writable_tools(workspace.path());
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    fs::write(workspace.path().join("README.md"), "grounding\n").expect("grounding file");
    let _ = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":10,"path":"README.md","start_line":1}"#,
    );

    let command = execute(
        &mut tools,
        "run_command",
        r#"{"args":[],"program":"peritus-definitely-missing-executable","purpose":"verification"}"#,
    );

    assert!(command.is_error);
    let result = wire(&command);
    assert!(result.contains("was not found through PATH"));
    assert!(result.contains("authorize ordinary dependency installation"));
    assert!(result.contains("retry the real command"));
    assert!(result.contains("Do not substitute a stand-in"));
}

#[test]
fn structured_commands_drain_and_bound_both_output_streams() {
    let workspace = tempfile::tempdir().expect("workspace");
    let mut tools = writable_tools(workspace.path());
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    fs::write(workspace.path().join(".peritus-output-fixture"), "run").expect("fixture marker");
    let executable = std::env::current_exe().expect("current test executable");
    let program = serde_json::to_string(&executable).expect("program path");
    let arguments = format!(
        r#"{{"args":["--exact","developer_tools::executor::tests::command::command_output_fixture","--nocapture"],"cwd":".","program":{program},"timeout_seconds":10}}"#
    );

    let command = execute(&mut tools, "run_command", &arguments);

    assert!(!command.is_error);
    let result: Value = serde_json::from_str(&wire(&command)).expect("command result JSON");
    assert_eq!(result["timed_out"].as_bool(), Some(false));
    assert!(result["recovery_hint"].is_null());
    assert!(result["stdout"].as_str().is_some_and(|value| value.contains("[output truncated]")));
    assert!(result["stderr"].as_str().is_some_and(|value| value.contains("[output truncated]")));
    assert!(
        result["stdout"]
            .as_str()
            .is_some_and(|value| value.contains("stdout-begin") && value.contains("stdout-final"))
    );
    assert!(
        result["stderr"]
            .as_str()
            .is_some_and(|value| value.contains("stderr-begin") && value.contains("stderr-final"))
    );
}

#[test]
fn command_timeout_fixture() {
    if !Path::new(".peritus-timeout-fixture").is_file() {
        return;
    }
    std::thread::sleep(Duration::from_secs(30));
}

#[test]
fn command_output_fixture() {
    if !Path::new(".peritus-output-fixture").is_file() {
        return;
    }
    let output = vec![b'x'; 600 * 1024];
    std::io::stdout().write_all(b"stdout-begin\n").expect("stdout prefix");
    std::io::stdout().write_all(&output).expect("fixture stdout");
    std::io::stdout().write_all(b"\nstdout-final").expect("stdout suffix");
    std::io::stderr().write_all(b"stderr-begin\n").expect("stderr prefix");
    std::io::stderr().write_all(&output).expect("fixture stderr");
    std::io::stderr().write_all(b"\nstderr-final").expect("stderr suffix");
}
