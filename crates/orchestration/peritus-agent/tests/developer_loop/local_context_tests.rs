//! Local context seam ordering, retention, and failure isolation.

use super::*;
use peritus_agent::{
    DeveloperContextAssembly, DeveloperContextEvent, DeveloperContextPort, DeveloperLoopError,
};
use peritus_model_protocol::{BoundedText, CompletedToolCall, Message};

/// In-memory test port only: durable host storage is a subsequent implementation package.
#[derive(Default)]
struct RecordingContext {
    view: Vec<Message>,
    raw_outputs: Vec<String>,
    invocations: Vec<String>,
    checkpoint: Vec<Message>,
    generations: usize,
    batches: usize,
    fail_at: Option<&'static str>,
    source_metadata: Option<CanonicalJson>,
    ignore_invocation_policy: bool,
}

impl RecordingContext {
    fn reject(&self, boundary: &str) -> Result<(), DeveloperLoopError> {
        if self.fail_at == Some(boundary) {
            return Err(DeveloperLoopError::Context(boundary.to_owned()));
        }
        Ok(())
    }
}

impl DeveloperContextPort for RecordingContext {
    fn source_reference(
        &self,
        _: &CompletedToolCall,
    ) -> Result<Option<CanonicalJson>, DeveloperLoopError> {
        Ok(self.source_metadata.clone())
    }
    fn open(
        &mut self,
        request: &DeveloperLoopRequest,
        initial: &[Message],
    ) -> Result<(), DeveloperLoopError> {
        self.reject("open")?;
        self.invocations.push(request.request_prefix.clone());
        if self.view.is_empty() {
            self.view = initial.to_vec();
        } else {
            self.view[0] = initial[0].clone();
            self.view.extend_from_slice(&initial[1..]);
        }
        Ok(())
    }

    fn observe(&mut self, event: DeveloperContextEvent<'_>) -> Result<(), DeveloperLoopError> {
        self.reject("observe")?;
        match event {
            DeveloperContextEvent::Message(message) => self.view.push(message.clone()),
            DeveloperContextEvent::ToolObservation { call, observation } => {
                self.reject("observation")?;
                assert_eq!(call.name().as_str(), "workspace_read");
                self.raw_outputs.push(observation.output.to_wire_string());
            }
            DeveloperContextEvent::BatchCompleted => self.batches += 1,
        }
        Ok(())
    }

    fn assemble(
        &mut self,
        request: DeveloperContextAssembly<'_>,
    ) -> Result<Vec<Message>, DeveloperLoopError> {
        self.reject("assemble")?;
        assert_eq!(request.tools.len(), 1);
        assert!(request.profile.limits().max_input_tokens() > 0);
        assert!(!request.messages.is_empty());
        let mut view = self.view.clone();
        if !self.ignore_invocation_policy {
            view[0] = request.invocation_policy.clone();
        }
        Ok(view)
    }

    fn checkpoint(&mut self, messages: &[Message]) -> Result<(), DeveloperLoopError> {
        self.reject("checkpoint")?;
        self.checkpoint = messages.to_vec();
        self.generations += 1;
        Ok(())
    }
}

#[path = "local_context_metadata.rs"]
mod metadata;

fn request(prefix: &str, turns: u16) -> DeveloperLoopRequest {
    DeveloperLoopRequest {
        request_prefix: prefix.to_owned(),
        system: "Inspect before completing.".to_owned(),
        prompt: "Read src/lib.rs and report.".to_owned(),
        attachments: Vec::new(),
        tools: vec![read_tool()],
        limits: DeveloperLoopLimits::new(turns, 4)
            .and_then(|limits| limits.with_max_attempts_per_turn(1))
            .and_then(|limits| limits.with_max_output_tokens(128))
            .expect("limits"),
        cancellation: CancellationToken::new(),
    }
}

fn provider(responses: VecDeque<VecDeque<EventEnvelope>>) -> ScriptedProvider {
    ScriptedProvider {
        profile: profile(),
        responses: Mutex::new(responses),
        requests: Mutex::new(Vec::new()),
    }
}

#[path = "local_context_lifecycle.rs"]
mod lifecycle;

#[test]
fn open_assembly_and_checkpoint_failures_stop_before_any_provider_request() {
    block_on(async {
        for boundary in ["open", "assemble", "checkpoint"] {
            let provider = provider(VecDeque::from([text_response()]));
            let mut memory = RecordingContext { fail_at: Some(boundary), ..Default::default() };
            let result = DeveloperLoop::run_with_context(
                &provider,
                request("failure", 1),
                &mut RecordingTool::default(),
                &mut RecordingTrace::default(),
                &mut memory,
            )
            .await;
            assert!(matches!(result, Err(DeveloperLoopError::Context(_))));
            assert!(provider.requests.lock().expect("requests").is_empty());
            assert_eq!(memory.generations, 0);
        }
    });
}

#[test]
fn proposal_persistence_failure_prevents_tool_execution() {
    block_on(async {
        let provider = provider(VecDeque::from([tool_response()]));
        let mut tools = RecordingTool::default();
        let mut memory = RecordingContext { fail_at: Some("observe"), ..Default::default() };
        let result = DeveloperLoop::run_with_context(
            &provider,
            request("proposal", 2),
            &mut tools,
            &mut RecordingTrace::default(),
            &mut memory,
        )
        .await;
        assert!(matches!(result, Err(DeveloperLoopError::Context(_))));
        assert_eq!(tools.calls, 0);
        assert_eq!(provider.requests.lock().expect("requests").len(), 1);
    });
}

#[test]
fn observation_ingestion_failure_preserves_trace_and_stops_the_tool_batch() {
    block_on(async {
        let mut provider = provider(VecDeque::from([batch_tool_response()]));
        provider.profile = parallel_profile();
        let mut tools = RecordingTool::default();
        let mut trace = RecordingTrace::default();
        let mut memory = RecordingContext { fail_at: Some("observation"), ..Default::default() };
        let result = DeveloperLoop::run_with_context(
            &provider,
            request("observation", 2),
            &mut tools,
            &mut trace,
            &mut memory,
        )
        .await;
        assert!(matches!(result, Err(DeveloperLoopError::Context(_))));
        assert_eq!(tools.calls, 1);
        assert_eq!(trace.observations, 1);
        assert_eq!(memory.batches, 0);
        assert_eq!(memory.view.last().expect("pending calls").role(), Role::Assistant);
    });
}

#[test]
fn oversized_local_view_never_selects_legacy_compaction_or_publishes() {
    block_on(async {
        let mut provider = provider(VecDeque::from([text_response()]));
        provider.profile = fixtures::constrained_profile();
        let mut memory = RecordingContext::default();
        let mut request = request("capacity", 1);
        request.prompt = "x".repeat(12_000);
        let result = DeveloperLoop::run_with_context(
            &provider,
            request,
            &mut RecordingTool::default(),
            &mut RecordingTrace::default(),
            &mut memory,
        )
        .await;
        assert!(matches!(result, Err(DeveloperLoopError::Context(_))));
        assert!(provider.requests.lock().expect("requests").is_empty());
        assert_eq!(memory.generations, 0);
        assert!(memory.checkpoint.is_empty());
    });
}

struct DiagnosticTool;

impl DeveloperToolExecutor for DiagnosticTool {
    fn execute(
        &mut self,
        _call: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        let output = format!(
            r#"{{"output":"{}DECISIVE_DIAGNOSTIC{}"}}"#,
            "x".repeat(60_000),
            "y".repeat(60_000)
        );
        Ok(DeveloperToolObservation {
            output: CanonicalJson::parse(&output, JsonBounds::value(ProtocolLimits::PRODUCTION))?,
            is_error: true,
        })
    }
}

#[test]
fn exact_diagnostic_is_ingested_before_model_visible_output_limiting() {
    block_on(async {
        let provider = provider(VecDeque::from([tool_response(), text_response()]));
        let mut memory = RecordingContext::default();
        let mut trace = RecordingTrace::default();
        DeveloperLoop::run_with_context(
            &provider,
            request("diagnostic", 2),
            &mut DiagnosticTool,
            &mut trace,
            &mut memory,
        )
        .await
        .expect("bounded view");
        assert!(memory.raw_outputs[0].contains("DECISIVE_DIAGNOSTIC"));
        assert_eq!(trace.observation_bytes[0], memory.raw_outputs[0].len());
        let requests = provider.requests.lock().expect("requests");
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|request| !matches!(request.tool_choice(), ToolChoice::None)));
        let result = requests[1]
            .messages()
            .iter()
            .flat_map(Message::content)
            .find_map(|block| {
                if let ContentBlock::ToolResult(result) = block { Some(result) } else { None }
            })
            .expect("tool result");
        assert!(result.output().canonical_bytes().len() < memory.raw_outputs[0].len());
        assert!(result.is_error());
        drop(requests);
    });
}

#[test]
fn local_context_keeps_host_completion_and_progress_corrections() {
    block_on(async {
        let provider =
            provider(VecDeque::from([text_response(), tool_response(), text_response()]));
        let mut memory = RecordingContext::default();
        DeveloperLoop::run_with_context(
            &provider,
            request("blocker", 3),
            &mut GroundingTool::default(),
            &mut RecordingTrace::default(),
            &mut memory,
        )
        .await
        .expect("grounding correction");
        assert!(memory.view.iter().flat_map(Message::content).any(|block| {
            matches!(block, ContentBlock::Text(text) if text.expose_for_wire().contains("cannot accept that terminal response"))
        }));
        let provider = self::provider(VecDeque::from([tool_response(), text_response()]));
        let mut memory = RecordingContext::default();
        DeveloperLoop::run_with_context(
            &provider,
            request("progress", 2),
            &mut ProgressFeedbackTool::default(),
            &mut RecordingTrace::default(),
            &mut memory,
        )
        .await
        .expect("progress correction");
        assert!(memory.view.iter().flat_map(Message::content).any(|block| {
            matches!(block, ContentBlock::Text(text) if text.expose_for_wire() == "take a concrete step now")
        }));
    });
}

#[test]
fn old_exchanges_above_the_legacy_trigger_remain_under_local_policy() {
    block_on(async {
        let provider = provider(VecDeque::from([text_response()]));
        let mut memory = RecordingContext::default();
        memory.view.push(
            Message::new(
                Role::System,
                vec![ContentBlock::Text(
                    BoundedText::new("policy".to_owned(), ProtocolLimits::PRODUCTION)
                        .expect("text"),
                )],
                ProtocolLimits::PRODUCTION,
            )
            .expect("message"),
        );
        for _ in 0..14 {
            memory.view.push(
                Message::new(
                    Role::User,
                    vec![ContentBlock::Text(
                        BoundedText::new("x".repeat(6_000), ProtocolLimits::PRODUCTION)
                            .expect("text"),
                    )],
                    ProtocolLimits::PRODUCTION,
                )
                .expect("message"),
            );
        }
        DeveloperLoop::run_with_context(
            &provider,
            request("trigger", 1),
            &mut RecordingTool::default(),
            &mut RecordingTrace::default(),
            &mut memory,
        )
        .await
        .expect("local view fits input capacity without subtracting output twice");
        let requests = provider.requests.lock().expect("requests");
        assert_eq!(requests.len(), 1);
        assert!(!matches!(requests[0].tool_choice(), ToolChoice::None));
        assert_eq!(requests[0].messages().len(), 16);
        drop(requests);
    });
}
