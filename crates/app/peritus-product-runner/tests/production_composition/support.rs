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

fn encoded_object(entries: Vec<(&str, Value)>) -> Vec<u8> {
    let object =
        entries.into_iter().map(|(key, value)| (key.to_owned(), value)).collect::<Map<_, _>>();
    serde_json::to_vec(&Value::Object(object)).expect("JSON arguments")
}

pub fn named_tool_response(name: &str, arguments: Vec<u8>) -> VecDeque<EventEnvelope> {
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
