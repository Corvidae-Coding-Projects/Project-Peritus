use super::*;
use peritus_agent::{DeveloperActivity, DeveloperInput, DeveloperInteraction, DeveloperLoopError};
use std::sync::atomic::{AtomicU64, Ordering};

struct LiveInput {
    revision: AtomicU64,
    applied: Mutex<Vec<u64>>,
    skipped: AtomicU64,
    cancel: Option<CancellationToken>,
}

impl DeveloperInteraction for LiveInput {
    fn input(&self) -> Result<DeveloperInput, DeveloperLoopError> {
        let revision = self.revision.load(Ordering::SeqCst);
        Ok(DeveloperInput {
            revision,
            conversation: if revision == 1 {
                "Inspect both files"
            } else {
                "Stop inspecting; discuss only"
            }
            .to_owned(),
        })
    }
    fn applied(&self, revision: u64) -> Result<(), DeveloperLoopError> {
        self.applied.lock().expect("applied").push(revision);
        Ok(())
    }
    fn observe(&self, activity: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError> {
        match activity {
            DeveloperActivity::ToolFinished { .. } => {
                self.revision.store(2, Ordering::SeqCst);
                if let Some(cancel) = &self.cancel {
                    assert!(cancel.cancel());
                }
            }
            DeveloperActivity::ToolSkipped { .. } => {
                self.skipped.fetch_add(1, Ordering::SeqCst);
            }
            _ => {}
        }
        Ok(())
    }
}

fn request(cancellation: CancellationToken) -> DeveloperLoopRequest {
    DeveloperLoopRequest {
        request_prefix: "interactive-boundary-test".to_owned(),
        system: "Respect steering".to_owned(),
        prompt: "Inspect both files".to_owned(),
        attachments: Vec::new(),
        tools: vec![read_tool()],
        limits: DeveloperLoopLimits::new(4, 8).expect("limits"),
        cancellation,
    }
}

#[test]
fn steering_skips_remaining_calls_and_reaches_the_next_model_request() {
    block_on(async {
        let provider = ScriptedProvider {
            profile: parallel_profile(),
            responses: Mutex::new(VecDeque::from([batch_tool_response(), text_response()])),
            requests: Mutex::new(Vec::new()),
        };
        let port = LiveInput {
            revision: AtomicU64::new(1),
            applied: Mutex::new(Vec::new()),
            skipped: AtomicU64::new(0),
            cancel: None,
        };
        let mut tools = RecordingTool::default();
        let mut trace = RecordingTrace::default();
        DeveloperLoop::run_interactive(
            &provider,
            request(CancellationToken::new()),
            &mut tools,
            &mut trace,
            None,
            &port,
        )
        .await
        .expect("conversation");
        assert_eq!(tools.calls, 1, "second old call must not execute");
        assert!(port.skipped.load(Ordering::SeqCst) > 0);
        assert_eq!(*port.applied.lock().expect("applied"), vec![1, 2]);
        let requests = provider.requests.lock().expect("requests").clone();
        assert!(requests.last().expect("new request").messages().iter().any(|message| message.content().iter().any(|block| {
            matches!(block, ContentBlock::Text(text) if text.expose_for_wire().contains("Stop inspecting; discuss only"))
        })));
    });
}

#[test]
fn cancellation_between_calls_prevents_the_remaining_effects() {
    block_on(async {
        let provider = ScriptedProvider {
            profile: parallel_profile(),
            responses: Mutex::new(VecDeque::from([batch_tool_response()])),
            requests: Mutex::new(Vec::new()),
        };
        let cancellation = CancellationToken::new();
        let port = LiveInput {
            revision: AtomicU64::new(1),
            applied: Mutex::new(Vec::new()),
            skipped: AtomicU64::new(0),
            cancel: Some(cancellation.clone()),
        };
        let mut tools = RecordingTool::default();
        let result = DeveloperLoop::run_interactive(
            &provider,
            request(cancellation),
            &mut tools,
            &mut RecordingTrace::default(),
            None,
            &port,
        )
        .await;
        assert!(matches!(result, Err(DeveloperLoopError::Cancelled)));
        assert_eq!(tools.calls, 1);
        assert_eq!(provider.requests.lock().expect("requests").len(), 1);
    });
}

struct SteeringRetryTrace<'a>(&'a LiveInput);
impl DeveloperTrace for SteeringRetryTrace<'_> {
    fn record(&mut self, event: DeveloperTraceEvent<'_>) -> Result<(), DeveloperLoopError> {
        if matches!(event, DeveloperTraceEvent::RetryScheduled(_)) {
            self.0.revision.store(2, Ordering::SeqCst);
        }
        Ok(())
    }
}

#[test]
fn input_received_during_retry_is_incorporated_before_the_next_provider_request() {
    block_on(async {
        let provider = ScriptedProvider {
            profile: parallel_profile(),
            responses: Mutex::new(VecDeque::from([
                fixtures::recoverable_failure_response(),
                text_response(),
            ])),
            requests: Mutex::new(Vec::new()),
        };
        let port = LiveInput {
            revision: AtomicU64::new(1),
            applied: Mutex::new(Vec::new()),
            skipped: AtomicU64::new(0),
            cancel: None,
        };
        DeveloperLoop::run_interactive(
            &provider,
            request(CancellationToken::new()),
            &mut RecordingTool::default(),
            &mut SteeringRetryTrace(&port),
            None,
            &port,
        )
        .await
        .expect("steered retry");
        assert_eq!(*port.applied.lock().expect("applied"), [1, 2]);
        let requests = provider.requests.lock().expect("requests").clone();
        assert_eq!(requests.len(), 2);
        assert!(requests[1].messages().iter().any(|message| message.content().iter().any(|block| {
            matches!(block, ContentBlock::Text(text) if text.expose_for_wire().contains("Stop inspecting; discuss only"))
        })));
    });
}
