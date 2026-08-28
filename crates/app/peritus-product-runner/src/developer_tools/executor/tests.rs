use super::*;
use peritus_model_protocol::{ToolCallId, ToolName};

#[test]
fn workspace_tools_inspect_edit_search_and_execute_without_a_shell() {
    let workspace = tempfile::tempdir().expect("workspace");
    let mut tools = WorkspaceDeveloperTools::new(workspace.path().to_owned());

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
    assert!(wire(&search).contains("src/lib.rs"));
    let listed = execute(&mut tools, "workspace_list", r#"{"depth":3,"path":""}"#);
    assert!(wire(&listed).contains("src/lib.rs"));

    let command =
        execute(&mut tools, "run_command", r#"{"args":["--version"],"cwd":".","program":"rustc"}"#);
    assert!(!command.is_error);
    assert!(wire(&command).contains(r#""success":true"#));
    let failed = execute(
        &mut tools,
        "run_command",
        r#"{"args":["--definitely-invalid"],"cwd":"","program":"rustc"}"#,
    );
    assert!(failed.is_error);
    assert!(wire(&failed).contains(r#""success":false"#));
}

#[test]
fn existing_files_cannot_be_mutated_before_they_are_read() {
    let workspace = tempfile::tempdir().expect("workspace");
    fs::write(workspace.path().join("README.md"), "before\n").expect("existing file");
    fs::write(workspace.path().join("Cargo.toml"), "[workspace]\n").expect("manifest");
    let mut tools = WorkspaceDeveloperTools::new(workspace.path().to_owned());
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
