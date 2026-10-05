//! Command deadlines and caller-selected runtime horizons.

use super::*;
use std::time::Instant;

#[test]
fn omitted_command_deadline_stays_absent_through_router_process_and_receipt() {
    let workspace = tempfile::tempdir().expect("workspace");
    let mut tools = WorkspaceDeveloperTools::with_ownership(
        workspace.path().to_owned(),
        WorkspaceOwnership::capture(workspace.path()),
        receipt_path(workspace.path()),
        "unbounded-command".to_owned(),
        None,
        test_command_runtime(workspace.path()),
    );
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    let command = execute(
        &mut tools,
        "run_command",
        r#"{"args":["--version"],"cwd":".","program":"rustc","purpose":"verification"}"#,
    );
    assert!(!command.is_error, "{}", wire(&command));
    let result: Value = serde_json::from_str(&wire(&command)).expect("command receipt");
    assert!(result["requested_timeout_seconds"].is_null());
    assert!(result["timeout_seconds"].is_null());
    assert_eq!(result["timed_out"], false);
    assert_eq!(result["deadline_limited"], false);
    for resource in result["tool_result"]["resources"].as_array().expect("resource observations") {
        if ["WallTimeMilliseconds", "CpuTimeMilliseconds"]
            .contains(&resource["dimension"].as_str().unwrap_or(""))
        {
            assert!(resource["ceiling"].is_null(), "{resource}");
        }
    }
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
        r#"{{"args":["--exact","developer_tools::executor::tests::command::command_timeout_fixture","--nocapture"],"cwd":".","program":{program},"purpose":"verification","timeout_seconds":1}}"#
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
fn command_timeouts_above_ten_minutes_reach_the_runtime_unchanged() {
    let workspace = tempfile::tempdir().expect("workspace");
    let mut tools = WorkspaceDeveloperTools::with_ownership(
        workspace.path().to_owned(),
        WorkspaceOwnership::capture(workspace.path()),
        receipt_path(workspace.path()),
        "unbounded-command-test".to_owned(),
        None,
        test_command_runtime(workspace.path()),
    );
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);

    let command = execute(
        &mut tools,
        "run_command",
        r#"{"args":["--version"],"cwd":".","program":"rustc","purpose":"verification","timeout_seconds":601}"#,
    );

    assert!(!command.is_error, "{}", wire(&command));
    let result: Value = serde_json::from_str(&wire(&command)).expect("command result JSON");
    assert_eq!(result["requested_timeout_seconds"].as_u64(), Some(601));
    assert_eq!(result["timeout_seconds"].as_u64(), Some(601));
    assert_eq!(result["deadline_limited"].as_bool(), Some(false));
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
        r#"{{"args":["--exact","developer_tools::executor::tests::command::command_timeout_fixture","--nocapture"],"cwd":".","program":{program},"purpose":"verification","timeout_seconds":10}}"#
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
