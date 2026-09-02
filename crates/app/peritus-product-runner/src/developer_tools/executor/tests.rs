use super::*;
use peritus_model_protocol::{CanonicalJson, JsonBounds, ProtocolLimits, ToolCallId, ToolName};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

mod access;
mod command;

static NEXT_CALL_ID: AtomicU64 = AtomicU64::new(1);

#[test]
fn workspace_tools_inspect_edit_search_and_execute_without_a_shell() {
    let workspace = tempfile::tempdir().expect("workspace");
    let mut tools = writable_tools(workspace.path());

    let initial = execute(&mut tools, "workspace_list", r#"{"depth":3,"path":""}"#);
    assert!(!initial.is_error);
    let initial_value: Value = serde_json::from_str(&wire(&initial)).expect("list result JSON");
    assert_eq!(
        initial_value["workspace_root"].as_str(),
        Some(workspace.path().to_string_lossy().as_ref()),
    );
    assert_eq!(initial_value["path_kind"], "workspace-relative");
    assert!(initial_value["execution_resources"]["logical_cpus"].as_u64().is_some());
    assert!(initial_value["execution_resources"]["effective_cpus"].as_u64().is_some());
    assert!(
        initial_value["execution_resources"]["recommended_parallelism"]
            .as_u64()
            .is_some_and(|value| value >= 1)
    );

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
        entries.iter().any(|entry| {
            entry["path"].as_str() == Some(expected_path.as_str())
                && entry["bytes"].as_u64().is_some()
                && entry["permissions"].as_str().is_some()
        })
    }));

    let command =
        execute(&mut tools, "run_command", r#"{"args":["--version"],"cwd":".","program":"rustc"}"#);
    assert!(!command.is_error);
    assert!(wire(&command).contains(r#""success":true"#));
    assert!(wire(&command).contains(r#""timed_out":false"#));
    let evidence = tools.verification_evidence();
    assert!(evidence.contains("[Developer command 1]"));
    assert!(evidence.contains(r#""program":"rustc""#));
    assert!(evidence.contains(r#""success":true"#));
    let failed = execute(
        &mut tools,
        "run_command",
        r#"{"args":["--definitely-invalid"],"cwd":"","program":"rustc"}"#,
    );
    assert!(failed.is_error);
    assert!(wire(&failed).contains(r#""success":false"#));
}

#[test]
fn task_contract_keeps_opaque_implementation_out_of_tool_evidence() {
    let workspace = tempfile::tempdir().expect("workspace");
    fs::write(workspace.path().join("forward.py"), "A1 = 'hidden'\n").expect("opaque input");
    fs::write(workspace.path().join("notes.txt"), "A1 must stay hidden\n").expect("notes");
    let mut tools = writable_tools(workspace.path()).with_task_contract(
        "Query by importing `forward.py` and calling forward(x). You do not know the shape of A1.",
    );

    let listed = execute(&mut tools, "workspace_list", r#"{"depth":2,"path":""}"#);
    assert!(!listed.is_error);
    let blocked_read = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":20,"path":"forward.py","start_line":1}"#,
    );
    assert!(blocked_read.is_error);
    assert!(wire(&blocked_read).contains("opaque query interface"));

    let search =
        execute(&mut tools, "workspace_search", r#"{"max_results":10,"path":"","query":"A1"}"#);
    let search_value: Value = serde_json::from_str(&wire(&search)).expect("search JSON");
    let matches = search_value["matches"].as_array().expect("matches");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["path"], "notes.txt");

    let blocked_command = execute(
        &mut tools,
        "run_command",
        r#"{"args":["-c","import forward; print(forward.A1)"],"program":"python3","purpose":"verification"}"#,
    );
    assert!(blocked_command.is_error);
    assert!(wire(&blocked_command).contains("hidden state"));
}

#[test]
fn long_inspection_sequence_returns_one_concrete_delivery_nudge() {
    let workspace = tempfile::tempdir().expect("workspace");
    fs::write(workspace.path().join("README.md"), "grounding\n").expect("grounding file");
    let mut tools = writable_tools(workspace.path());
    let _ = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    let _ = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":10,"path":"README.md","start_line":1}"#,
    );
    for _ in 0..10 {
        let result = execute(
            &mut tools,
            "workspace_search",
            r#"{"max_results":5,"path":".","query":"not-present"}"#,
        );
        assert!(!result.is_error);
    }

    let feedback = tools.take_progress_feedback().expect("progress feedback");
    assert!(feedback.contains("shortest concrete delivery step"));
    assert!(feedback.contains("package or runtime manager"));
    assert!(feedback.contains("before hand-writing a substitute"));
    assert!(tools.take_progress_feedback().is_none());
}

#[cfg(unix)]
#[test]
fn workspace_reads_report_exact_posix_permissions() {
    use std::os::unix::fs::PermissionsExt as _;

    let workspace = tempfile::tempdir().expect("workspace");
    let path = workspace.path().join("private.key");
    fs::write(&path, "secret\n").expect("private key");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("private permissions");
    let mut tools = writable_tools(workspace.path());

    let listed = execute(&mut tools, "workspace_list", r#"{"depth":1,"path":""}"#);
    let list: Value = serde_json::from_str(&wire(&listed)).expect("list JSON");
    let key = list["entries"]
        .as_array()
        .and_then(|entries| entries.iter().find(|entry| entry["path"] == "private.key"))
        .expect("listed key");
    assert_eq!(key["permissions"], "0600");

    let read = execute(
        &mut tools,
        "workspace_read",
        r#"{"end_line":10,"path":"private.key","start_line":1}"#,
    );
    let value: Value = serde_json::from_str(&wire(&read)).expect("read JSON");
    assert_eq!(value["permissions"], "0600");
    assert_eq!(value["bytes"], 7);
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

fn execute(
    tools: &mut WorkspaceDeveloperTools,
    name: &str,
    arguments: &str,
) -> DeveloperToolObservation {
    let call = completed_call(
        &format!("{name}-call-{}", NEXT_CALL_ID.fetch_add(1, Ordering::Relaxed)),
        name,
        arguments,
    );
    tools.execute(&call).expect("tool dispatch")
}

fn completed_call(id: &str, name: &str, arguments: &str) -> CompletedToolCall {
    CompletedToolCall::new(
        ToolCallId::new(id.to_owned()).expect("call ID"),
        ToolName::new(name.to_owned()).expect("tool name"),
        CanonicalJson::parse(arguments, JsonBounds::value(ProtocolLimits::PRODUCTION))
            .expect("arguments"),
    )
    .expect("completed call")
}

fn wire(observation: &DeveloperToolObservation) -> String {
    String::from_utf8(observation.output.canonical_bytes().to_vec()).expect("JSON")
}

fn writable_tools(root: &Path) -> WorkspaceDeveloperTools {
    writable_tools_with_horizon(root, Duration::from_secs(30))
}

fn writable_tools_with_horizon(root: &Path, horizon: Duration) -> WorkspaceDeveloperTools {
    WorkspaceDeveloperTools::with_ownership(
        root.to_owned(),
        WorkspaceOwnership::capture(root),
        receipt_path(root),
        "writer-test".to_owned(),
        horizon,
        test_command_runtime(root),
    )
}

fn receipt_path(root: &Path) -> PathBuf {
    root.join(".git/peritus-test-effects.bin")
}
