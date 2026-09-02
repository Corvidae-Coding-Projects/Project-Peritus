use super::*;
use std::{
    io::{BufRead as _, Write as _},
    process::Command,
    time::Instant,
};

#[test]
fn active_commands_accept_terminal_input_and_reach_a_stable_result() {
    let workspace = tempfile::tempdir().expect("workspace");
    let mut tools = writable_tools(workspace.path());
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    fs::write(workspace.path().join(".peritus-stdin-fixture"), "run").expect("fixture marker");
    let executable = std::env::current_exe().expect("current test executable");
    let program = serde_json::to_string(&executable).expect("program path");
    let start = execute(
        &mut tools,
        "command_start",
        &format!(
            r#"{{"args":["--exact","developer_tools::executor::tests::command::command_stdin_fixture","--nocapture"],"cwd":".","interactive":true,"program":{program},"purpose":"verification","timeout_seconds":10}}"#
        ),
    );
    assert!(!start.is_error, "{}", wire(&start));
    let started: Value = serde_json::from_str(&wire(&start)).expect("start result");
    let handle = started["handle"].as_str().expect("command handle");
    let input = execute(
        &mut tools,
        "command_stdin",
        &format!(r#"{{"handle":"{handle}","text":"hello from peritus\n"}}"#),
    );
    assert!(!input.is_error, "{}", wire(&input));
    let terminal = poll_terminal(&mut tools, handle);
    assert_eq!(terminal["status"], "succeeded");
    assert!(
        terminal["stdout"]
            .as_str()
            .is_some_and(|output| output.contains("received:hello from peritus")),
        "{terminal}"
    );
    let repeated = execute(&mut tools, "command_poll", &format!(r#"{{"handle":"{handle}"}}"#));
    assert!(!repeated.is_error, "{}", wire(&repeated));
    let repeated: Value = serde_json::from_str(&wire(&repeated)).expect("repeated terminal result");
    assert_eq!(repeated, terminal);
    let successful = tools.successful_commands();
    assert_eq!(successful.len(), 1);
    assert_eq!(successful[0].purpose, crate::developer_tools::CommandPurpose::Verification);
    assert!(successful[0].command.starts_with("command_start "));
}

#[test]
fn active_commands_cancel_the_owned_process_tree() {
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
    let started: Value = serde_json::from_str(&wire(&start)).expect("start result");
    let handle = started["handle"].as_str().expect("command handle");
    let cancelled = execute(&mut tools, "command_cancel", &format!(r#"{{"handle":"{handle}"}}"#));
    assert!(!cancelled.is_error || wire(&cancelled).contains("cancelled"));
    let terminal = poll_terminal(&mut tools, handle);
    assert_eq!(terminal["status"], "cancelled");
}

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
    assert_eq!(result["timed_out"].as_bool(), Some(true), "{}", wire(&command));
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
    let recovery = result["recovery_hint"]
        .as_str()
        .unwrap_or_else(|| panic!("deadline recovery: {}", wire(&command)));
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
    assert!(result.contains("authorize ordinary dependency installation"), "{result}");
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
    assert!(
        result["stdout"].as_str().is_some_and(|value| value.contains("[output truncated]")),
        "{}",
        wire(&command)
    );
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
fn command_created_files_can_be_removed_without_owning_unrelated_late_files() {
    let workspace = tempfile::tempdir().expect("workspace");
    assert!(
        Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(workspace.path())
            .status()
            .expect("git init")
            .success()
    );
    fs::write(workspace.path().join("README.md"), "grounding\n").expect("grounding file");
    let mut tools = writable_tools(workspace.path());
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    let _ = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":10,"path":"README.md","start_line":1}"#,
    );

    fs::write(workspace.path().join("late-external.log"), "preserve\n")
        .expect("late external file");
    fs::write(workspace.path().join(".peritus-create-output-fixture"), "run")
        .expect("fixture marker");
    let executable = std::env::current_exe().expect("current test executable");
    let program = serde_json::to_string(&executable).expect("program path");
    let arguments = format!(
        r#"{{"args":["--exact","developer_tools::executor::tests::command::command_created_file_fixture","--nocapture"],"cwd":".","program":{program},"timeout_seconds":10}}"#
    );

    let command = execute(&mut tools, "run_command", &arguments);
    assert!(!command.is_error, "{}", wire(&command));
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    for path in ["command-output.txt", "late-external.log"] {
        let _ = execute(
            &mut tools,
            "workspace_read",
            &format!(r#"{{"end_line":10,"path":"{path}","start_line":1}}"#),
        );
    }

    let command_output =
        execute(&mut tools, "workspace_remove", r#"{"path":"command-output.txt"}"#);
    assert!(!command_output.is_error, "{}", wire(&command_output));
    assert!(!workspace.path().join("command-output.txt").exists());

    let external = execute(&mut tools, "workspace_remove", r#"{"path":"late-external.log"}"#);
    assert!(external.is_error);
    assert!(wire(&external).contains("externally produced evidence"));
    assert!(workspace.path().join("late-external.log").is_file());
}

#[test]
fn active_command_created_files_are_owned_after_terminal_observation() {
    let workspace = tempfile::tempdir().expect("workspace");
    fs::write(workspace.path().join("README.md"), "grounding\n").expect("grounding file");
    let mut tools = writable_tools(workspace.path());
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    let _ = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":10,"path":"README.md","start_line":1}"#,
    );
    fs::write(workspace.path().join(".peritus-create-output-fixture"), "run")
        .expect("fixture marker");
    let executable = std::env::current_exe().expect("current test executable");
    let program = serde_json::to_string(&executable).expect("program path");
    let start = execute(
        &mut tools,
        "command_start",
        &format!(
            r#"{{"args":["--exact","developer_tools::executor::tests::command::command_created_file_fixture","--nocapture"],"cwd":".","interactive":false,"program":{program},"purpose":"verification","timeout_seconds":10}}"#
        ),
    );
    let started: Value = serde_json::from_str(&wire(&start)).expect("start result");
    let handle = started["handle"].as_str().expect("command handle");
    let terminal = poll_terminal(&mut tools, handle);
    assert_eq!(terminal["status"], "succeeded");

    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    let _ = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":10,"path":"command-output.txt","start_line":1}"#,
    );
    let removed = execute(&mut tools, "workspace_remove", r#"{"path":"command-output.txt"}"#);
    assert!(!removed.is_error, "{}", wire(&removed));
    assert!(!workspace.path().join("command-output.txt").exists());
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

#[test]
fn command_created_file_fixture() {
    if !Path::new(".peritus-create-output-fixture").is_file() {
        return;
    }
    fs::write("command-output.txt", "generated by command\n").expect("command output");
}

#[test]
fn command_stdin_fixture() {
    if !Path::new(".peritus-stdin-fixture").is_file() {
        return;
    }
    let mut input = String::new();
    std::io::stdin().lock().read_line(&mut input).expect("fixture stdin");
    let output = format!("received:{}\n", input.trim());
    std::io::stdout().write_all(output.as_bytes()).expect("fixture stdout");
}

fn poll_terminal(tools: &mut WorkspaceDeveloperTools, handle: &str) -> Value {
    for _ in 0..200 {
        let observation = execute(tools, "command_poll", &format!(r#"{{"handle":"{handle}"}}"#));
        let value: Value = serde_json::from_str(&wire(&observation)).expect("poll result");
        if value["state"] == "completed" {
            return value;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("active command did not terminate")
}
