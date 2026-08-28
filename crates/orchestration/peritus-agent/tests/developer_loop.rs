//! Production D0 developer-loop integration with a scripted provider and concrete tool port.

use std::{collections::VecDeque, future::Future, sync::Mutex};

use peritus_agent::{
    DeveloperLoop, DeveloperLoopLimits, DeveloperLoopRequest, DeveloperToolExecutor,
    DeveloperToolObservation, DeveloperTrace, DeveloperTraceEvent,
};
use peritus_model_protocol::{
    BoundedText, CancellationKind, CanonicalJson, Capability, CapabilityMatrix,
    CapabilityProvenance, ContentBlock, EventEnvelope, FinishReason, ItemId, ItemKind, JsonBounds,
    JsonSchema, ModelEvent, ModelLimits, ModelName, ModelRequest, OutputLimitEnforcement,
    ProtocolLimits, ProviderName, ProviderProfile, ResumeKind, Role, SchemaDialect, StateMode,
    StreamFragment, ToolCallId, ToolDefinition, ToolName, WireDialect,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, ModelStream, OwnedModelStream, ProviderCoreError,
};
use peritus_types::{ProviderProfileId, Sha256Digest};

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

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

struct ScriptedProvider {
    profile: ProviderProfile,
    responses: Mutex<VecDeque<VecDeque<EventEnvelope>>>,
    requests: Mutex<Vec<ModelRequest>>,
}

impl ModelProvider for ScriptedProvider {
    fn profile(&self) -> &ProviderProfile {
        &self.profile
    }

    fn start(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        Box::pin(async move {
            self.requests
                .lock()
                .map_err(|_| ProviderCoreError::configuration("scripted_provider", "lock failed"))?
                .push(request);
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

#[derive(Default)]
struct RecordingTool {
    calls: u32,
}

impl DeveloperToolExecutor for RecordingTool {
    fn execute(
        &mut self,
        call: &peritus_model_protocol::CompletedToolCall,
    ) -> Result<DeveloperToolObservation, peritus_agent::DeveloperLoopError> {
        assert_eq!(call.name().as_str(), "workspace_read");
        self.calls += 1;
        Ok(DeveloperToolObservation {
            output: CanonicalJson::parse(
                r#"{"content":"pub fn answer() -> u32 { 42 }"}"#,
                JsonBounds::value(ProtocolLimits::PRODUCTION),
            )?,
            is_error: false,
        })
    }
}

#[derive(Default)]
struct RecordingTrace {
    envelopes: u32,
    observations: u32,
}

impl DeveloperTrace for RecordingTrace {
    fn record(
        &mut self,
        event: DeveloperTraceEvent<'_>,
    ) -> Result<(), peritus_agent::DeveloperLoopError> {
        match event {
            DeveloperTraceEvent::ProviderEnvelope(_) => self.envelopes += 1,
            DeveloperTraceEvent::ToolObservation { .. } => self.observations += 1,
        }
        Ok(())
    }
}

#[test]
fn developer_loop_executes_a_tool_and_returns_its_observation_to_the_next_model_turn() {
    block_on(async {
        let provider = ScriptedProvider {
            profile: profile(),
            responses: Mutex::new(VecDeque::from([tool_response(), text_response()])),
            requests: Mutex::new(Vec::new()),
        };
        let mut tools = RecordingTool::default();
        let mut trace = RecordingTrace::default();
        let outcome = DeveloperLoop::run(
            &provider,
            DeveloperLoopRequest {
                request_prefix: "developer-test".to_owned(),
                system: "Inspect before completing.".to_owned(),
                prompt: "Read src/lib.rs and report.".to_owned(),
                tools: vec![read_tool()],
                limits: DeveloperLoopLimits::new(4, 4).expect("limits"),
                cancellation: CancellationToken::new(),
            },
            &mut tools,
            &mut trace,
        )
        .await
        .expect("developer loop");

        assert_eq!(outcome.text, "implementation inspected");
        assert_eq!(outcome.model_turns, 2);
        assert_eq!(outcome.tool_calls, 1);
        assert_eq!(tools.calls, 1);
        assert_eq!(trace.observations, 1);
        assert!(trace.envelopes > 0);
        let requests = provider.requests.lock().expect("requests");
        assert_eq!(requests.len(), 2);
        assert!(requests[1].messages().iter().any(|message| {
            message.role() == Role::Tool
                && message
                    .content()
                    .iter()
                    .any(|block| matches!(block, ContentBlock::ToolResult(_)))
        }));
        drop(requests);
    });
}

fn profile() -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new([0x7A; 16]).expect("profile ID"),
        1,
        ProviderName::new("scripted-provider".to_owned()).expect("provider"),
        ModelName::new("scripted-model".to_owned()).expect("model"),
        WireDialect::CompatibleResponses,
        CapabilityMatrix::new(&[Capability::ToolCalls], &[]).expect("capabilities"),
        CapabilityProvenance::Probed,
        ModelLimits::new(32_768, 4_096, 16, 1, 256 * 1024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile")
}

fn read_tool() -> ToolDefinition {
    let limits = ProtocolLimits::PRODUCTION;
    ToolDefinition::new(
        ToolName::new("workspace_read".to_owned()).expect("tool name"),
        Some(BoundedText::new("Read a workspace file".to_owned(), limits).expect("description")),
        JsonSchema::parse(
            r#"{"additionalProperties":false,"properties":{"path":{"type":"string"}},"required":["path"],"type":"object"}"#,
            SchemaDialect::Draft202012,
            JsonBounds::schema(limits),
        )
        .expect("schema"),
        true,
    )
}

fn tool_response() -> VecDeque<EventEnvelope> {
    let limits = ProtocolLimits::PRODUCTION;
    let item = ItemId::new("tool-item".to_owned()).expect("item");
    let call = ToolCallId::new("read-call".to_owned()).expect("call");
    response([
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind: ItemKind::ToolCall },
        ModelEvent::ToolCallStarted {
            item_id: item.clone(),
            call_id: call.clone(),
            name: ToolName::new("workspace_read".to_owned()).expect("tool"),
        },
        ModelEvent::ToolArgumentDelta {
            call_id: call,
            fragment: StreamFragment::new(br#"{"path":"src/lib.rs"}"#.to_vec(), limits)
                .expect("arguments"),
        },
        ModelEvent::ItemCompleted(item),
        ModelEvent::Finish(FinishReason::ToolCalls),
        ModelEvent::ResponseCompleted,
    ])
}

fn text_response() -> VecDeque<EventEnvelope> {
    let limits = ProtocolLimits::PRODUCTION;
    let item = ItemId::new("message-item".to_owned()).expect("item");
    response([
        ModelEvent::ResponseStarted { response_id: None, model: None },
        ModelEvent::ItemStarted { item_id: item.clone(), index: 0, kind: ItemKind::Message },
        ModelEvent::TextDelta {
            item_id: item.clone(),
            fragment: StreamFragment::new(b"implementation inspected".to_vec(), limits)
                .expect("text"),
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
