use std::fs;

use peritus_agent::{DeveloperToolExecutor, DeveloperToolObservation};
use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, JsonBounds, ProtocolLimits, ToolCallId, ToolName,
};

use super::WorkspaceDeveloperTools;
use crate::developer_tools::WorkspaceOwnership;

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
    );
    let replay = recovered.execute(&call).expect("receipt replay");

    assert_eq!(wire(&replay), wire(&original));
    assert_eq!(
        fs::read_to_string(workspace.path().join("artifact.txt")).expect("preserved external file"),
        "external\n",
    );
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
