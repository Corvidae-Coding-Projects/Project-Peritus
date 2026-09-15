//! Shared exact local-memory test fixtures.

use super::super::{
    LocalContextConfig,
    assembly::text_message,
    memory::LocalMemory,
    record::{self, CheckpointManifest, MemoryRecord},
};
use peritus_agent::{DeveloperLoopLimits, DeveloperLoopRequest, DeveloperToolObservation};
use peritus_codec::sha256;
use peritus_context::{
    ContextNodeId,
    working::{WorkingBinding, encode_working_state},
};
use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, ContentBlock, JsonBounds, Message, ProtocolLimits, Role,
    ToolCallId, ToolName, ToolResult, encode_messages,
};
use peritus_provider_core::CancellationToken;
use peritus_role::HarnessRole;
use peritus_types::{RunId, WorkspaceId};
use serde_json::Value;

pub(in crate::local_context) struct Fixture {
    pub(in crate::local_context) state: tempfile::TempDir,
    pub(in crate::local_context) workspace: tempfile::TempDir,
}
impl Fixture {
    pub(in crate::local_context) fn new() -> Self {
        let workspace = tempfile::tempdir().unwrap();
        std::fs::write(workspace.path().join("input.txt"), "initial").unwrap();
        for arguments in [
            vec!["init", "--quiet"],
            vec!["add", "input.txt"],
            vec![
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "--quiet",
                "-m",
                "fixture",
            ],
        ] {
            let output = std::process::Command::new("git")
                .args(arguments)
                .current_dir(workspace.path())
                .output()
                .unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        }
        Self { state: tempfile::tempdir().unwrap(), workspace }
    }
    pub(in crate::local_context) fn open(&self) -> LocalMemory {
        LocalMemory::load(
            &self.state.path().join("memory"),
            self.workspace.path(),
            &self.state.path().join("run.trace"),
            binding(),
            LocalContextConfig::default(),
        )
        .unwrap()
    }
}
pub(in crate::local_context) fn binding() -> WorkingBinding {
    WorkingBinding::new(
        RunId::new([1; 16]).unwrap(),
        WorkspaceId::new([2; 16]).unwrap(),
        ContextNodeId::new([3; 16]).unwrap(),
        HarnessRole::Writer,
        1,
    )
}
pub(in crate::local_context) fn begin(memory: &mut LocalMemory, prefix: &str) {
    let request = DeveloperLoopRequest {
        request_prefix: prefix.to_owned(),
        system: "Immutable policy".to_owned(),
        prompt: "Inspect input.txt".to_owned(),
        attachments: vec![],
        tools: vec![],
        limits: DeveloperLoopLimits::new(4, 16).unwrap(),
        cancellation: CancellationToken::new(),
    };
    memory
        .begin(
            &request,
            &[message(Role::System, &request.system), message(Role::User, &request.prompt)],
        )
        .unwrap();
}
pub(in crate::local_context) fn message(role: Role, text: &str) -> Message {
    text_message(role, text.to_owned()).unwrap()
}
pub(in crate::local_context) fn call(id: &str) -> CompletedToolCall {
    CompletedToolCall::new(
        ToolCallId::new(id.to_owned()).unwrap(),
        ToolName::new("workspace_read".to_owned()).unwrap(),
        canonical(&Value::from_iter([("path", Value::from("input.txt"))])),
    )
    .unwrap()
}
pub(in crate::local_context) fn canonical(value: &Value) -> CanonicalJson {
    CanonicalJson::parse(&value.to_string(), JsonBounds::value(ProtocolLimits::PRODUCTION)).unwrap()
}
pub(in crate::local_context) fn proposal(memory: &mut LocalMemory, call: &CompletedToolCall) {
    memory
        .observe_message(
            &Message::new(
                Role::Assistant,
                vec![ContentBlock::ToolCall(call.clone())],
                ProtocolLimits::PRODUCTION,
            )
            .unwrap(),
        )
        .unwrap();
}
pub(in crate::local_context) fn observation(
    memory: &mut LocalMemory,
    id: &str,
    text: &str,
    is_error: bool,
) -> u64 {
    let call = call(id);
    proposal(memory, &call);
    let observation = DeveloperToolObservation {
        output: canonical(&Value::from_iter([("diagnostic", Value::from(text))])),
        is_error,
    };
    let source = memory.observe_tool(&call, &observation).unwrap();
    memory
        .observe_message(
            &Message::new(
                Role::Tool,
                vec![ContentBlock::ToolResult(ToolResult::new(
                    call.id().clone(),
                    observation.output,
                    is_error,
                ))],
                ProtocolLimits::PRODUCTION,
            )
            .unwrap(),
        )
        .unwrap();
    source
}
pub(in crate::local_context) fn update(
    revision: u64,
    source: u64,
    label: &str,
    text: &str,
) -> Value {
    Value::from_iter([
        ("base_revision", Value::from(revision)),
        (
            "operations",
            Value::Array(vec![Value::from_iter([
                ("id", Value::from(label)),
                ("kind", Value::from("hypothesis")),
                ("text", Value::from(text)),
                ("supports", Value::Array(vec![Value::from(format!("obs:{source:06}"))])),
                ("contradicts", Value::Array(vec![])),
                ("depends_on", Value::Array(vec![])),
                ("status", Value::from("open")),
                ("validity", Value::from("candidate")),
                ("files", Value::Array(vec![])),
                ("supersedes", Value::Null),
            ])]),
        ),
    ])
}
pub(in crate::local_context) fn render(messages: &[Message]) -> String {
    messages
        .iter()
        .flat_map(Message::content)
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.expose_for_wire()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
pub(in crate::local_context) fn profile(capacity: u64) -> peritus_model_protocol::ProviderProfile {
    use peritus_model_protocol::{
        CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, ModelLimits,
        ModelName, OutputLimitEnforcement, ProviderName, ProviderProfile, ResumeKind, StateMode,
        WireDialect,
    };
    ProviderProfile::new(
        peritus_types::ProviderProfileId::new([7; 16]).unwrap(),
        1,
        ProviderName::new("local-fixture".to_owned()).unwrap(),
        ModelName::new("local-fixture".to_owned()).unwrap(),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&[Capability::ToolCalls], &[]).unwrap(),
        CapabilityProvenance::Probed,
        ModelLimits::new(capacity, 128, 32, 1, 512 * 1024).unwrap(),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .unwrap()
}

pub(in crate::local_context) fn publish_legacy_v1(memory: &mut LocalMemory, messages: &[Message]) {
    let prepared = memory.prepared.as_ref().expect("prepared legacy view");
    assert_eq!(prepared.messages, messages);
    assert_eq!(prepared.through_event, memory.store.sequence());
    let through_event = prepared.through_event;
    let render_policy = prepared.policy.into_bytes();
    let mut view_validation = prepared.validation.clone();
    view_validation.tool_policy = None;

    let working_state = memory
        .store
        .store(&encode_working_state(&memory.state).expect("encode legacy working state"))
        .expect("store legacy working state");
    let transcript_manifest = memory
        .store
        .store(&record::encode(&memory.transcript).expect("encode legacy transcript"))
        .expect("store legacy transcript");
    let source_index = memory
        .store
        .store(&record::encode(&memory.sources).expect("encode legacy sources"))
        .expect("store legacy sources");
    let view = memory
        .store
        .store(&encode_messages(messages, ProtocolLimits::PRODUCTION).expect("encode legacy view"))
        .expect("store legacy view");
    let validation_bytes = record::encode(&view_validation).expect("encode legacy validation");
    assert!(!String::from_utf8_lossy(&validation_bytes).contains("tool_policy"));
    let validation = memory.store.store(&validation_bytes).expect("store legacy validation");
    let previous = memory.last_checkpoint.as_ref().map(|manifest| {
        sha256(&record::encode(manifest).expect("encode legacy predecessor")).into_bytes()
    });
    let manifest = CheckpointManifest {
        schema_version: record::LEGACY_CHECKPOINT_SCHEMA_VERSION,
        scope: memory.store.scope_digest().into_bytes(),
        generation: memory.store.generation().checked_add(1).expect("legacy generation"),
        previous,
        through_event,
        working_state,
        transcript_manifest,
        source_index,
        view,
        render_policy,
        validation,
        view_binding: None,
    };
    let bytes = record::encode(&manifest).expect("encode legacy manifest");
    assert!(!String::from_utf8_lossy(&bytes).contains("view_binding"));
    append_manifest(memory, &manifest, bytes);
    memory.last_checkpoint = Some(manifest);
    memory.last_view = messages.to_vec();
    memory.prepared = None;
}

pub(in crate::local_context) fn append_manifest(
    memory: &mut LocalMemory,
    manifest: &CheckpointManifest,
    bytes: Vec<u8>,
) {
    let artifact = memory.store.store(&bytes).expect("store checkpoint manifest");
    let event = record::encode(&MemoryRecord::Checkpoint { manifest: artifact })
        .expect("encode checkpoint event");
    let roots = [
        manifest.working_state.digest,
        manifest.transcript_manifest.digest,
        manifest.source_index.digest,
        manifest.view.digest,
        manifest.validation.digest,
        artifact.digest,
    ];
    let generation = memory.store.generation();
    memory
        .store
        .append(&event, &roots, Some((generation, bytes)))
        .expect("append checkpoint manifest");
}
