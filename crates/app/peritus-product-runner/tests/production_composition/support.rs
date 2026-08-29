use std::{collections::VecDeque, path::Path, sync::Mutex};

use peritus_model_protocol::{
    CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance, EventEnvelope,
    FinishReason, ItemId, ItemKind, ModelEvent, ModelLimits, ModelName, ModelRequest,
    OutputLimitEnforcement, ProtocolLimits, ProviderName, ProviderProfile, ResumeKind, StateMode,
    StreamFragment, ToolCallId, ToolName, WireDialect,
};
use peritus_product_runner::ConversationView;
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, ModelStream, OwnedModelStream, ProviderCoreError,
};
use peritus_types::{ProviderProfileId, Sha256Digest};
use serde_json::{Map, Value};

struct ScriptedStream {
    events: VecDeque<EventEnvelope>,
}

impl ModelStream for ScriptedStream {
    fn next<'a>(
        &'a mut self,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move { Ok(self.events.pop_front()) })
    }
}

pub struct ScriptedProvider {
    pub profile: ProviderProfile,
    pub responses: Mutex<VecDeque<VecDeque<EventEnvelope>>>,
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
            Ok(OwnedModelStream::new(ScriptedStream { events }, cancellation))
        })
    }
}

pub struct FixedConversation(pub String);

impl ConversationView for FixedConversation {
    fn revision(&self) -> u64 {
        1
    }

    fn render(&self) -> String {
        self.0.clone()
    }
}

pub fn profile(id: [u8; 16], name: &str) -> ProviderProfile {
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

pub fn tool_response(arguments: Vec<u8>) -> VecDeque<EventEnvelope> {
    named_tool_response("workspace_write", arguments)
}

pub fn write_arguments(path: &str, content: &str) -> Vec<u8> {
    encoded_object(vec![
        ("path", Value::String(path.to_owned())),
        ("content", Value::String(content.to_owned())),
    ])
}

pub fn patch_arguments(path: &str, old: &str, new: &str, replace_all: bool) -> Vec<u8> {
    encoded_object(vec![
        ("path", Value::String(path.to_owned())),
        ("old", Value::String(old.to_owned())),
        ("new", Value::String(new.to_owned())),
        ("replace_all", Value::Bool(replace_all)),
    ])
}

pub fn list_arguments(path: &str, depth: usize) -> Vec<u8> {
    encoded_object(vec![("path", Value::String(path.to_owned())), ("depth", Value::from(depth))])
}

pub fn read_arguments(path: &str) -> Vec<u8> {
    encoded_object(vec![
        ("path", Value::String(path.to_owned())),
        ("start_line", Value::from(1)),
        ("end_line", Value::from(500)),
    ])
}

fn encoded_object(entries: Vec<(&str, Value)>) -> Vec<u8> {
    let object =
        entries.into_iter().map(|(key, value)| (key.to_owned(), value)).collect::<Map<_, _>>();
    serde_json::to_vec(&Value::Object(object)).expect("JSON arguments")
}

pub fn named_tool_response(name: &str, arguments: Vec<u8>) -> VecDeque<EventEnvelope> {
    named_tool_response_with_id(name, name, arguments)
}

pub fn named_tool_response_with_id(
    name: &str,
    id: &str,
    arguments: Vec<u8>,
) -> VecDeque<EventEnvelope> {
    let limits = ProtocolLimits::PRODUCTION;
    let item = ItemId::new(format!("{name}-{id}-item")).expect("item");
    let call = ToolCallId::new(format!("{name}-{id}-call")).expect("call");
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

pub fn text_response(text: &[u8]) -> VecDeque<EventEnvelope> {
    let limits = ProtocolLimits::PRODUCTION;
    let item = ItemId::new(format!("text-{}", text.len())).expect("item");
    response([
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind: ItemKind::Message },
        ModelEvent::TextDelta {
            item_id: item.clone(),
            fragment: StreamFragment::new(text.to_vec(), limits).expect("text"),
        },
        ModelEvent::ItemCompleted(item),
        ModelEvent::Finish(FinishReason::Stop),
        ModelEvent::ResponseCompleted,
    ])
}

pub fn design_response() -> VecDeque<EventEnvelope> {
    text_response(br"# Tested answer implementation design

## Objective and acceptance criteria
Add the requested answer API as a maintained public Rust function. The implementation must return exactly 42, include focused regression coverage, compile without warnings, and preserve the existing package shape and unrelated source.

## Repository findings
The repository is a single Cargo package rooted at `Cargo.toml`. Its implementation is in `src/lib.rs`, which already exposes a small constant function and is the correct location for the new API. There are no separate integration-test or application crates to coordinate.

## Architecture and interfaces
Keep the change inside the library's existing public API. Add a documented `answer() -> u32` constant function and an adjacent unit-test module. No persistence, networking, process, or cross-crate interface is involved.

## Data and control flow
The caller invokes `answer`; the function returns the compile-time integer constant 42 with no input, allocation, mutation, or external effect. The unit test calls that exact public function and compares its result with 42.

## File and module plan
Modify only `src/lib.rs`. Preserve the existing item unless the task requires replacement, add rustdoc and `#[must_use]`, and keep the focused test beside the implementation.

## Implementation slices
First inspect the existing module. Then add the documented API and its regression test as one coherent change. Finally run the package checks and address any compiler or lint output.

## Verification
Require Cargo check, tests, and Clippy for all targets and features. Acceptance requires the regression test to assert 42 and every exact-target command to exit successfully.

## Risks and non-goals
The realistic risk is an implementation and test that agree on the wrong value, so the assertion must independently name 42. No unrelated redesign or speculative hardening is in scope.
")
}

fn response<const N: usize>(events: [ModelEvent; N]) -> VecDeque<EventEnvelope> {
    events
        .into_iter()
        .enumerate()
        .map(|(index, event)| {
            let sequence = u64::try_from(index + 1).expect("sequence");
            EventEnvelope::new(
                sequence,
                None,
                None,
                Sha256Digest::new([u8::try_from(index + 1).expect("digest byte"); 32]),
                event,
            )
            .expect("envelope")
        })
        .collect()
}

pub fn git(root: &Path, arguments: &[&str]) {
    let output =
        std::process::Command::new("git").args(arguments).current_dir(root).output().expect("git");
    assert!(output.status.success(), "git failed: {}", String::from_utf8_lossy(&output.stderr));
}

pub fn cargo(root: &Path, arguments: &[&str]) {
    let output = std::process::Command::new("cargo")
        .args(arguments)
        .current_dir(root)
        .output()
        .expect("cargo");
    assert!(output.status.success(), "cargo failed: {}", String::from_utf8_lossy(&output.stderr));
}
