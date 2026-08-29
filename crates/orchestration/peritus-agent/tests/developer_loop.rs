//! Production D0 developer-loop integration with a scripted provider and concrete tool port.

#[path = "developer_loop/fixtures.rs"]
mod fixtures;

use std::{collections::VecDeque, future::Future, sync::Mutex};

use fixtures::{
    batch_tool_response, empty_response, parallel_profile, profile, read_tool,
    recoverable_failure_response, text_response, tool_response,
};
use peritus_agent::{
    DeveloperLoop, DeveloperLoopLimits, DeveloperLoopRequest, DeveloperToolExecutor,
    DeveloperToolObservation, DeveloperTrace, DeveloperTraceEvent,
};
use peritus_model_protocol::{
    CanonicalJson, ContentBlock, EventEnvelope, JsonBounds, ModelRequest, ParallelToolPolicy,
    ProtocolLimits, ProviderProfile, Role,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, ModelProvider, ModelStream, OwnedModelStream, ProviderCoreError,
};

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
                attachments: Vec::new(),
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
        assert_eq!(requests[0].parallel_tool_policy(), ParallelToolPolicy::Disabled);
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

#[test]
fn developer_loop_uses_the_negotiated_parallel_tool_width() {
    block_on(async {
        let provider = ScriptedProvider {
            profile: parallel_profile(),
            responses: Mutex::new(VecDeque::from([batch_tool_response(), text_response()])),
            requests: Mutex::new(Vec::new()),
        };
        let mut tools = RecordingTool::default();
        let mut trace = RecordingTrace::default();
        let outcome = DeveloperLoop::run(
            &provider,
            DeveloperLoopRequest {
                request_prefix: "parallel-test".to_owned(),
                system: "Inspect both files before completing.".to_owned(),
                prompt: "Read src/lib.rs and src/main.rs.".to_owned(),
                attachments: Vec::new(),
                tools: vec![read_tool()],
                limits: DeveloperLoopLimits::new(4, 4).expect("limits"),
                cancellation: CancellationToken::new(),
            },
            &mut tools,
            &mut trace,
        )
        .await
        .expect("developer loop");

        assert_eq!(outcome.tool_calls, 2);
        assert_eq!(tools.calls, 2);
        assert_eq!(trace.observations, 2);
        let requests = provider.requests.lock().expect("requests");
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].parallel_tool_policy(), ParallelToolPolicy::Allowed(4));
        let observations = requests[1]
            .messages()
            .iter()
            .flat_map(peritus_model_protocol::Message::content)
            .filter(|block| matches!(block, ContentBlock::ToolResult(_)))
            .count();
        assert_eq!(observations, 2);
        drop(requests);
    });
}

#[test]
fn developer_loop_retries_a_recoverable_malformed_provider_turn() {
    block_on(async {
        let provider = ScriptedProvider {
            profile: profile(),
            responses: Mutex::new(VecDeque::from([
                recoverable_failure_response(),
                empty_response(),
                text_response(),
            ])),
            requests: Mutex::new(Vec::new()),
        };
        let mut tools = RecordingTool::default();
        let mut trace = RecordingTrace::default();
        let outcome = DeveloperLoop::run(
            &provider,
            DeveloperLoopRequest {
                request_prefix: "recovery-test".to_owned(),
                system: "Complete the task.".to_owned(),
                prompt: "Return the result.".to_owned(),
                attachments: Vec::new(),
                tools: vec![read_tool()],
                limits: DeveloperLoopLimits::new(4, 4).expect("limits"),
                cancellation: CancellationToken::new(),
            },
            &mut tools,
            &mut trace,
        )
        .await
        .expect("developer loop recovers");

        assert_eq!(outcome.text, "implementation inspected");
        assert_eq!(outcome.model_turns, 1);
        assert_eq!(provider.requests.lock().expect("requests").len(), 3);
    });
}
