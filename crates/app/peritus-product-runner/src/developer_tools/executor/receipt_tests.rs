use std::{
    fs::{self, OpenOptions},
    io::Write as _,
    time::Duration,
};

use peritus_agent::{DeveloperToolExecutor, DeveloperToolObservation};
use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, JsonBounds, ProtocolLimits, ToolCallId, ToolName,
};

use super::WorkspaceDeveloperTools;
use crate::developer_tools::WorkspaceOwnership;

#[path = "receipt_tests/disconnect.rs"]
mod disconnect;

#[test]
fn completed_workspace_effect_replays_without_a_second_write() {
    let workspace = tempfile::tempdir().expect("workspace");
    let receipts = workspace.path().join(".git/peritus-test-effects.bin");
    let scope = "writer-replay";
    let call = completed_call(
        "stable-call",
        "workspace_write",
        r#"{"content":"agent\n","path":"artifact.txt"}"#,
    );
    let mut first = WorkspaceDeveloperTools::with_ownership(
        workspace.path().to_owned(),
        WorkspaceOwnership::capture(workspace.path()),
        receipts.clone(),
        scope.to_owned(),
        Duration::from_secs(30),
        super::test_command_runtime(workspace.path()),
    );
    let list = completed_call("list", "workspace_list", r#"{"depth":1,"path":""}"#);
    let _ = first.execute(&list).expect("workspace list");
    let original = first.execute(&call).expect("first effect");
    assert!(!original.is_error);
    fs::write(workspace.path().join("artifact.txt"), "external\n").expect("external change");

    let mut recovered = WorkspaceDeveloperTools::with_ownership(
        workspace.path().to_owned(),
        first.ownership().clone(),
        receipts,
        scope.to_owned(),
        Duration::from_secs(30),
        first.command_runtime.as_ref().expect("command runtime").clone(),
    );
    let replay = recovered.execute(&call).expect("receipt replay");

    assert_eq!(wire(&replay), wire(&original));
    assert_eq!(
        fs::read_to_string(workspace.path().join("artifact.txt")).expect("preserved external file"),
        "external\n",
    );
}

#[test]
fn reused_provider_call_id_cannot_dispatch_a_second_external_effect() {
    let workspace = tempfile::tempdir().expect("workspace");
    let receipts = workspace.path().join(".git/peritus-test-effects.bin");
    let mut tools = WorkspaceDeveloperTools::with_ownership(
        workspace.path().to_owned(),
        WorkspaceOwnership::capture(workspace.path()),
        receipts,
        "writer-reused-call".to_owned(),
        Duration::from_secs(30),
        super::test_command_runtime(workspace.path()),
    );
    let list = completed_call("list", "workspace_list", r#"{"depth":1,"path":""}"#);
    let _ = tools.execute(&list).expect("workspace list");
    let arguments = serde_json::json!({
        "args": [
            "--exact",
            "developer_tools::executor::receipt_tests::append_effect_counter_fixture",
            "--ignored",
        ],
        "cwd": ".",
        "program": std::env::current_exe().expect("test executable"),
        "purpose": "external_effect",
    })
    .to_string();
    let call = completed_call("reused-call", "run_command", &arguments);

    let executed = tools.execute(&call).expect("first external effect");
    let refused = tools.execute(&call).expect("refused duplicate provider ID");

    assert!(!executed.is_error, "{}", wire(&executed));
    assert_eq!(
        fs::read_to_string(workspace.path().join("effect-count")).expect("effect counter"),
        "effect\n",
        "a reused provider ID must not dispatch the same external effect twice",
    );
    assert!(refused.is_error);
    assert!(wire(&refused).contains("more than one effect request"));
}

#[test]
#[ignore = "subprocess fixture invoked by the duplicate-effect regression"]
fn append_effect_counter_fixture() {
    let path = std::env::current_dir().expect("working directory").join("effect-count");
    let mut file = OpenOptions::new().create(true).append(true).open(path).expect("effect counter");
    file.write_all(b"effect\n").expect("append effect");
    file.sync_data().expect("persist effect");
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
