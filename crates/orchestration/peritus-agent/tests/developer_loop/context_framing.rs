//! Context pressure must survive a real framing rejection on its compaction request.

use super::*;
use peritus_provider_core::{FramingLimits, ProviderCoreErrorKind, SseParser};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct FramingProvider {
    inner: ScriptedProvider,
    malformed: bool,
    framing_failures: Arc<AtomicUsize>,
}

impl ModelProvider for FramingProvider {
    fn profile(&self) -> &ProviderProfile {
        self.inner.profile()
    }

    fn start(
        &self,
        request: ModelRequest,
        cancellation: CancellationToken,
    ) -> BoxFuture<'_, Result<OwnedModelStream, ProviderCoreError>> {
        if self.malformed && matches!(request.tool_choice(), ToolChoice::None) {
            self.inner.requests.lock().expect("requests").push(request);
            let failures = Arc::clone(&self.framing_failures);
            Box::pin(async move {
                Ok(OwnedModelStream::new(MalformedCompaction(failures), cancellation))
            })
        } else {
            self.inner.start(request, cancellation)
        }
    }
}

struct MalformedCompaction(Arc<AtomicUsize>);

impl ModelStream for MalformedCompaction {
    fn next<'a>(
        &'a mut self,
        _: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<Option<EventEnvelope>, ProviderCoreError>> {
        Box::pin(async move {
            // The incomplete code point is legal while buffered; completing the CRLF line
            // must reject it. No hand-written provider diagnostic substitutes for the parser.
            let mut parser = SseParser::new(FramingLimits::new(16, 32).expect("limits"));
            assert!(parser.push(b"data: \xf0\x9f\r").expect("buffered fragment").is_empty());
            let error = parser.push(b"\n\n").expect_err("incomplete UTF-8 at line boundary");
            assert_eq!(error.kind(), ProviderCoreErrorKind::MalformedStream);
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(error)
        })
    }
}

#[derive(Default)]
struct ObservationTrace {
    inner: RecordingTrace,
    outputs: Vec<Vec<u8>>,
}

impl DeveloperTrace for ObservationTrace {
    fn record(
        &mut self,
        event: DeveloperTraceEvent<'_>,
    ) -> Result<(), peritus_agent::DeveloperLoopError> {
        if let DeveloperTraceEvent::ToolObservation { observation, .. } = &event {
            self.outputs.push(observation.output.canonical_bytes().to_vec());
        }
        self.inner.record(event)
    }
}

#[test]
fn context_pressure_with_malformed_compaction_framing_preserves_completed_work() {
    block_on(async {
        // Controls prove that pressure reaches compaction and that a usable reply installs
        // a semantic checkpoint. Framing fails only on that causally required request.
        for (pressure, malformed) in [(false, true), (true, false), (true, true)] {
            check_case(pressure, malformed).await;
        }
    });
}

async fn check_case(pressure: bool, malformed: bool) {
    let provider = FramingProvider {
        inner: ScriptedProvider {
            profile: profile(),
            responses: Mutex::new(VecDeque::from([
                if pressure { oversized_tool_argument_response() } else { tool_response() },
                text_response(),
            ])),
            requests: Mutex::new(Vec::new()),
        },
        malformed,
        framing_failures: Arc::new(AtomicUsize::new(0)),
    };
    let caller = CancellationToken::new();
    let mut tools = RecordingTool::default();
    let mut trace = ObservationTrace::default();
    let outcome = DeveloperLoop::run(
        &provider,
        DeveloperLoopRequest {
            request_prefix: "context-framing-pair".to_owned(),
            system: "Inspect before completing.".to_owned(),
            prompt: "Read the workspace and complete the task.".to_owned(),
            attachments: Vec::new(),
            tools: vec![read_tool()],
            limits: DeveloperLoopLimits::new(3, 3)
                .and_then(|limits| limits.with_max_attempts_per_turn(1))
                .expect("limits"),
            cancellation: caller.clone(),
        },
        &mut tools,
        &mut trace,
    )
    .await
    .expect("framing failure falls back within the same invocation");
    assert_eq!(outcome.text, "implementation inspected");
    assert_eq!(tools.calls, 1);
    assert_eq!(trace.outputs, [br#"{"content":"pub fn answer() -> u32 { 42 }"}"#.to_vec()]);
    assert!(!caller.is_cancelled());
    assert_eq!(
        provider.framing_failures.load(Ordering::SeqCst),
        usize::from(pressure && malformed)
    );
    let requests = provider.inner.requests.lock().expect("requests");
    assert_eq!(requests.len(), if pressure { 3 } else { 2 });
    assert_eq!(
        requests.iter().filter(|request| matches!(request.tool_choice(), ToolChoice::None)).count(),
        usize::from(pressure),
    );
    let final_request = requests.last().expect("final developer request");
    assert_text_request_fits(final_request, provider.profile().limits().max_input_tokens());
    let checkpoints: Vec<_> = final_request
        .messages()
        .iter()
        .flat_map(peritus_model_protocol::Message::content)
        .filter_map(|block| match block {
            ContentBlock::Text(text)
                if text.expose_for_wire().starts_with("<peritus-compaction ") =>
            {
                Some(text.expose_for_wire())
            }
            _ => None,
        })
        .collect();
    assert_eq!(checkpoints.len(), usize::from(pressure));
    if pressure {
        assert_eq!(checkpoints[0].contains("mode=\"semantic\""), !malformed);
        assert!(checkpoints[0].contains("source_sha256="));
    }
    drop(requests);
    assert_eq!(u32::from(outcome.compactions), u32::from(pressure));
}

fn assert_text_request_fits(request: &ModelRequest, capacity: u64) {
    // Independently itemize this text/tool-only fixture's contract: message overhead, exact
    // payloads, tool schema/name/description overhead, then fixed request framing and ceil / 3.
    let mut bytes = 512_usize;
    for message in request.messages() {
        bytes += 16;
        for block in message.content() {
            bytes += match block {
                ContentBlock::Text(text) => text.expose_for_wire().len(),
                ContentBlock::ToolCall(call) => {
                    call.arguments().canonical_bytes().len()
                        + call.id().expose_for_wire().len()
                        + call.name().as_str().len()
                }
                ContentBlock::ToolResult(result) => {
                    result.output().canonical_bytes().len()
                        + result.call_id().expose_for_wire().len()
                }
                _ => panic!("unexpected fixture content"),
            };
        }
    }
    for tool in request.tools() {
        bytes += 32
            + tool.name().as_str().len()
            + tool.parameters().canonical_bytes().len()
            + tool.description().map_or(0, |text| text.expose_for_wire().len());
    }
    assert!(u64::try_from(bytes.div_ceil(3)).expect("bounded estimate") <= capacity);
}
