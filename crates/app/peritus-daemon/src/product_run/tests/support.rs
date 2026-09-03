//! Scripted model and repository fixture owned by daemon product-run tests.

use std::{
    collections::VecDeque,
    fs,
    path::Path,
    sync::{Arc, Mutex},
};

use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, EventEnvelope,
    FinishReason, ItemId, ItemKind, ModelEvent, ModelLimits, ModelName, ModelRequest,
    OutputLimitEnforcement, ProtocolLimits, ProviderName, ProviderProfile, ResumeKind, StateMode,
    StreamFragment, ToolCallId, ToolName, WireDialect,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, ModelStream, OwnedModelStream, ProviderCoreError,
};
use peritus_types::{ProviderProfileId, Sha256Digest};
use serde_json::{Map, Value};

pub(super) const CORRECT: &str = "pub const fn answer() -> u32 {\n    42\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn answer_is_42() {\n        assert_eq!(super::answer(), 42);\n    }\n}\n";

struct ScriptedStream {
    events: VecDeque<EventEnvelope>,
    stall_on_empty: bool,
}

impl ModelStream for ScriptedStream {
    fn next<'a>(
        &'a mut self,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move {
            if let Some(event) = self.events.pop_front() {
                return Ok(Some(event));
            }
            if self.stall_on_empty {
                cancellation.cancelled().await;
            }
            Ok(None)
        })
    }
}

pub(super) struct ScriptedProvider {
    pub(super) profile: ProviderProfile,
    pub(super) responses: Mutex<VecDeque<VecDeque<EventEnvelope>>>,
    stalled: bool,
}

impl ModelProvider for ScriptedProvider {
    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    fn start(
        &self,
        _request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async move {
            let events = self
                .responses
                .lock()
                .map_err(|_| ProviderCoreError::configuration("scripted_provider", "lock failed"))?
                .pop_front()
                .ok_or_else(|| {
                    ProviderCoreError::configuration("scripted_provider", "script exhausted")
                })?;
            Ok(OwnedModelStream::new(
                ScriptedStream { events, stall_on_empty: self.stalled },
                cancellation,
            ))
        })
    }
}

pub(super) fn scripted(
    id: u8,
    name: &str,
    responses: Vec<VecDeque<EventEnvelope>>,
) -> Arc<ScriptedProvider> {
    Arc::new(ScriptedProvider {
        profile: profile([id; 16], name),
        responses: Mutex::new(responses.into()),
        stalled: false,
    })
}

pub(super) fn stalled(id: u8, name: &str) -> Arc<ScriptedProvider> {
    Arc::new(ScriptedProvider {
        profile: profile([id; 16], name),
        responses: Mutex::new(VecDeque::from([response([ModelEvent::ResponseStarted {
            response_id: None,
            model: None,
        }])])),
        stalled: true,
    })
}

pub(super) fn complete_writer(source: &str) -> Vec<VecDeque<EventEnvelope>> {
    vec![
        named_tool_response("workspace_list", list_arguments("", 3)),
        named_tool_response("workspace_read", read_arguments("Cargo.toml")),
        named_tool_response("workspace_read", read_arguments("src/lib.rs")),
        design_response(),
        named_tool_response("workspace_list", list_arguments("", 3)),
        named_tool_response("workspace_read", read_arguments("src/lib.rs")),
        named_tool_response("workspace_write", write_arguments("src/lib.rs", source)),
        text_response(
            br#"{"kind":"complete","run_instructions":"cargo test","summary":"Implemented the requested answer."}"#,
        ),
    ]
}

pub(super) fn clean_review() -> Vec<VecDeque<EventEnvelope>> {
    vec![
        named_tool_response("workspace_list", list_arguments("", 3)),
        named_tool_response("workspace_read", read_arguments("src/lib.rs")),
        text_response(br#"{"findings":[],"summary":"The exact request and gates are satisfied."}"#),
    ]
}

pub(super) fn repository() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("repository");
    fs::create_dir_all(root.path().join("src")).expect("source directory");
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"resume-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("manifest");
    fs::write(root.path().join("src/lib.rs"), "pub const fn before() -> u32 { 1 }\n")
        .expect("source");
    command(root.path(), "git", &["init", "--quiet"]);
    command(root.path(), "git", &["config", "user.name", "Peritus Test"]);
    command(root.path(), "git", &["config", "user.email", "peritus@example.invalid"]);
    command(root.path(), "git", &["config", "commit.gpgsign", "false"]);
    command(root.path(), "cargo", &["generate-lockfile"]);
    command(root.path(), "git", &["add", "."]);
    command(root.path(), "git", &["commit", "--quiet", "-m", "initial"]);
    root
}

fn profile(id: [u8; 16], name: &str) -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new(id).expect("profile ID"),
        1,
        ProviderName::new(format!("scripted-{name}")).expect("provider"),
        ModelName::new(format!("scripted-{name}")).expect("model"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&[Capability::ToolCalls], &[]).expect("capabilities"),
        CapabilityProvenance::Probed,
        ModelLimits::new(32_768, 4_096, 16, 1, 512 * 1024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile")
}

fn design_response() -> VecDeque<EventEnvelope> {
    text_response(br"# Tested answer implementation design

## Objective and acceptance criteria
Add the requested answer API as a maintained public Rust function. It must return exactly 42, include focused regression coverage, compile without warnings, and preserve unrelated source.

## Repository findings
The repository is one Cargo package. Its implementation is in src/lib.rs, which is the correct location for this small public API.

## Architecture and interfaces
Add a documented answer() -> u32 constant function and adjacent unit test without introducing another subsystem.

## Data and control flow
The caller invokes answer and receives the integer 42 without allocation, mutation, or external effects.

## File and module plan
Modify only src/lib.rs and preserve the existing item.

## Implementation slices
Inspect the module, add the API and regression test, then run exact package checks.

## Verification
Require Cargo check, tests, and Clippy. The regression test must independently assert 42.

## Risks and non-goals
The realistic risk is a matching wrong implementation and test. No unrelated redesign is in scope.
")
}

fn list_arguments(path: &str, depth: usize) -> Vec<u8> {
    encoded_object(vec![("path", Value::String(path.to_owned())), ("depth", Value::from(depth))])
}

fn read_arguments(path: &str) -> Vec<u8> {
    encoded_object(vec![
        ("path", Value::String(path.to_owned())),
        ("start_line", Value::from(1)),
        ("end_line", Value::from(500)),
    ])
}

fn write_arguments(path: &str, content: &str) -> Vec<u8> {
    encoded_object(vec![
        ("path", Value::String(path.to_owned())),
        ("content", Value::String(content.to_owned())),
    ])
}

fn encoded_object(entries: Vec<(&str, Value)>) -> Vec<u8> {
    let object =
        entries.into_iter().map(|(key, value)| (key.to_owned(), value)).collect::<Map<_, _>>();
    serde_json::to_vec(&Value::Object(object)).expect("JSON arguments")
}

fn named_tool_response(name: &str, arguments: Vec<u8>) -> VecDeque<EventEnvelope> {
    let limits = ProtocolLimits::PRODUCTION;
    let item = ItemId::new(format!("{name}-item")).expect("item");
    let call = ToolCallId::new(format!("{name}-call")).expect("call");
    response([
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind: ItemKind::ToolCall },
        ModelEvent::ToolCallStarted {
            item_id: item.clone(),
            call_id: call.clone(),
            name: ToolName::new(name.to_owned()).expect("tool"),
        },
        ModelEvent::ToolArgumentDelta {
            call_id: call,
            fragment: StreamFragment::new(arguments, limits).expect("arguments"),
        },
        ModelEvent::ItemCompleted(item),
        ModelEvent::Finish(FinishReason::ToolCalls),
        ModelEvent::ResponseCompleted,
    ])
}

fn text_response(text: &[u8]) -> VecDeque<EventEnvelope> {
    let item = ItemId::new(format!("text-{}", text.len())).expect("item");
    response([
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind: ItemKind::Message },
        ModelEvent::TextDelta {
            item_id: item.clone(),
            fragment: StreamFragment::new(text.to_vec(), ProtocolLimits::PRODUCTION).expect("text"),
        },
        ModelEvent::ItemCompleted(item),
        ModelEvent::Finish(FinishReason::Stop),
        ModelEvent::ResponseCompleted,
    ])
}

fn response<const N: usize>(events: [ModelEvent; N]) -> VecDeque<EventEnvelope> {
    events
        .into_iter()
        .enumerate()
        .map(|(index, event)| {
            EventEnvelope::new(
                u64::try_from(index + 1).expect("sequence"),
                None,
                None,
                Sha256Digest::new([u8::try_from(index + 1).expect("digest byte"); 32]),
                event,
            )
            .expect("envelope")
        })
        .collect()
}

fn command(root: &Path, executable: &str, arguments: &[&str]) {
    let output = std::process::Command::new(executable)
        .args(arguments)
        .current_dir(root)
        .output()
        .expect("fixture command");
    assert!(
        output.status.success(),
        "{executable} failed: {}",
        String::from_utf8_lossy(&output.stderr),
    );
}
