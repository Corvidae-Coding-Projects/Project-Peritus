use super::*;
use peritus_model_protocol::{ToolCallId, ToolName};
use std::{
    io::Write as _,
    path::Path,
    time::{Duration, Instant},
};

#[test]
fn workspace_tools_inspect_edit_search_and_execute_without_a_shell() {
    let workspace = tempfile::tempdir().expect("workspace");
    let mut tools = writable_tools(workspace.path());

    let initial = execute(&mut tools, "workspace_list", r#"{"depth":3,"path":""}"#);
    assert!(!initial.is_error);

    let written = execute(
        &mut tools,
        "workspace_write",
        r#"{"content":"pub fn answer() -> u32 { 41 }\n","path":"src/lib.rs"}"#,
    );
    assert!(!written.is_error);
    let _ = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":10,"path":"src/lib.rs","start_line":1}"#,
    );
    let patched =
        execute(&mut tools, "workspace_patch", r#"{"new":"42","old":"41","path":"src/lib.rs"}"#);
    assert!(!patched.is_error);
    let read = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":10,"path":"src/lib.rs","start_line":1}"#,
    );
    assert!(wire(&read).contains("answer() -> u32 { 42 }"));
    let search = execute(
        &mut tools,
        "workspace_search",
        r#"{"max_results":10,"path":"src","query":"answer"}"#,
    );
    let expected_path = Path::new("src").join("lib.rs").to_string_lossy().into_owned();
    let search_value: Value = serde_json::from_str(&wire(&search)).expect("search result JSON");
    assert_eq!(search_value["matches"][0]["path"].as_str(), Some(expected_path.as_str()));
    let listed = execute(&mut tools, "workspace_list", r#"{"depth":3,"path":""}"#);
    let list_value: Value = serde_json::from_str(&wire(&listed)).expect("list result JSON");
    assert!(list_value["entries"].as_array().is_some_and(|entries| {
        entries.iter().any(|entry| entry["path"].as_str() == Some(expected_path.as_str()))
    }));

    let command =
        execute(&mut tools, "run_command", r#"{"args":["--version"],"cwd":".","program":"rustc"}"#);
    assert!(!command.is_error);
    assert!(wire(&command).contains(r#""success":true"#));
    assert!(wire(&command).contains(r#""timed_out":false"#));
    let failed = execute(
        &mut tools,
        "run_command",
        r#"{"args":["--definitely-invalid"],"cwd":"","program":"rustc"}"#,
    );
    assert!(failed.is_error);
    assert!(wire(&failed).contains(r#""success":false"#));
}

#[test]
fn structured_commands_time_out_without_freezing_the_agent() {
    let workspace = tempfile::tempdir().expect("workspace");
    let mut tools = writable_tools(workspace.path());
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    let executable = std::env::current_exe().expect("current test executable");
    let arguments = serde_json::to_string(&serde_json::json!({
        "args": ["--ignored", "--nocapture", "command_timeout_fixture"],
        "cwd": ".",
        "program": executable,
        "timeout_seconds": 1,
    }))
    .expect("command arguments");

    let started = Instant::now();
    let command = execute(&mut tools, "run_command", &arguments);

    assert!(command.is_error);
    assert!(started.elapsed() < Duration::from_secs(10));
    let result: Value = serde_json::from_str(&wire(&command)).expect("command result JSON");
    assert_eq!(result["success"].as_bool(), Some(false));
    assert_eq!(result["timed_out"].as_bool(), Some(true));
    assert_eq!(result["timeout_seconds"].as_u64(), Some(1));
}

#[test]
fn structured_commands_drain_and_bound_both_output_streams() {
    let workspace = tempfile::tempdir().expect("workspace");
    let mut tools = writable_tools(workspace.path());
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    let executable = std::env::current_exe().expect("current test executable");
    let arguments = serde_json::to_string(&serde_json::json!({
        "args": ["--ignored", "--nocapture", "command_output_fixture"],
        "cwd": ".",
        "program": executable,
        "timeout_seconds": 10,
    }))
    .expect("command arguments");

    let command = execute(&mut tools, "run_command", &arguments);

    assert!(!command.is_error);
    let result: Value = serde_json::from_str(&wire(&command)).expect("command result JSON");
    assert_eq!(result["timed_out"].as_bool(), Some(false));
    assert!(result["stdout"].as_str().is_some_and(|value| value.contains("[output truncated]")));
    assert!(result["stderr"].as_str().is_some_and(|value| value.contains("[output truncated]")));
}

#[test]
#[ignore = "launched by the structured-command timeout regression"]
fn command_timeout_fixture() {
    std::thread::sleep(Duration::from_secs(30));
}

#[test]
#[ignore = "launched by the structured-command output regression"]
fn command_output_fixture() {
    let output = vec![b'x'; 600 * 1024];
    std::io::stdout().write_all(&output).expect("fixture stdout");
    std::io::stderr().write_all(&output).expect("fixture stderr");
}

#[test]
fn workspace_write_reports_and_preserves_an_unchanged_file() {
    let workspace = tempfile::tempdir().expect("workspace");
    let path = workspace.path().join("artifact.txt");
    fs::write(&path, "stable\n").expect("existing file");
    let mut tools = writable_tools(workspace.path());
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":2,"path":""}"#);
    let _ = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":20,"path":"artifact.txt","start_line":1}"#,
    );
    let before = fs::metadata(&path).expect("metadata").modified().expect("modified");

    let unchanged =
        execute(&mut tools, "workspace_write", r#"{"content":"stable\n","path":"artifact.txt"}"#);

    assert!(!unchanged.is_error);
    assert!(wire(&unchanged).contains(r#""changed":false"#));
    assert_eq!(fs::read_to_string(&path).expect("file"), "stable\n");
    assert_eq!(fs::metadata(&path).expect("metadata").modified().expect("modified"), before);
}

#[test]
fn existing_files_cannot_be_mutated_before_they_are_read() {
    let workspace = tempfile::tempdir().expect("workspace");
    fs::write(workspace.path().join("README.md"), "before\n").expect("existing file");
    fs::write(workspace.path().join("Cargo.toml"), "[workspace]\n").expect("manifest");
    let mut tools = writable_tools(workspace.path());
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":2,"path":""}"#);
    let _ = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":20,"path":"Cargo.toml","start_line":1}"#,
    );

    let refused = execute(
        &mut tools,
        "workspace_patch",
        r#"{"new":"after","old":"before","path":"README.md"}"#,
    );
    assert!(refused.is_error);
    assert!(wire(&refused).contains("read the existing target"));

    let _ = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":20,"path":"README.md","start_line":1}"#,
    );
    let applied = execute(
        &mut tools,
        "workspace_patch",
        r#"{"new":"after","old":"before","path":"README.md"}"#,
    );
    assert!(!applied.is_error);
}

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
        r#"{"args":["api_access.log"],"cwd":".","program":"rm"}"#,
    );
    assert!(shell_delete.is_error);
    assert!(wire(&shell_delete).contains("workspace_remove"));
    assert!(workspace.path().join("api_access.log").is_file());

    let ownership = tools.ownership().clone();
    let mut fixer = WorkspaceDeveloperTools::with_ownership(workspace.path().to_owned(), ownership);
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

fn execute(
    tools: &mut WorkspaceDeveloperTools,
    name: &str,
    arguments: &str,
) -> DeveloperToolObservation {
    let call = CompletedToolCall::new(
        ToolCallId::new(format!("{name}-call")).expect("call ID"),
        ToolName::new(name.to_owned()).expect("tool name"),
        CanonicalJson::parse(arguments, JsonBounds::value(ProtocolLimits::PRODUCTION))
            .expect("arguments"),
    )
    .expect("completed call");
    tools.execute(&call).expect("tool dispatch")
}

fn wire(observation: &DeveloperToolObservation) -> String {
    String::from_utf8(observation.output.canonical_bytes().to_vec()).expect("JSON")
}

fn writable_tools(root: &Path) -> WorkspaceDeveloperTools {
    WorkspaceDeveloperTools::with_ownership(root.to_owned(), WorkspaceOwnership::capture(root))
}
