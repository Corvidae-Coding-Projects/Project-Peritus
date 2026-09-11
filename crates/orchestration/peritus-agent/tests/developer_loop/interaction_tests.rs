use super::*;
use peritus_agent::{DeveloperActivity, DeveloperInput, DeveloperInteraction, DeveloperLoopError};
use peritus_model_protocol::{CompletedToolCall, Message};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "interaction_tests/images.rs"]
mod images;
#[path = "interaction_tests/tool_identity.rs"]
mod tool_identity;

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
            images: Vec::new(),
            conversation: if revision == 1 {
                "Inspect both files"
            } else {
                "Stop inspecting; discuss only"
            }
            .to_owned(),
        })
    }
    fn prepare_request(
        &self,
        revision: u64,
        _: &ModelRequest,
    ) -> Result<peritus_agent::DeveloperRequestAdmission, DeveloperLoopError> {
        if self.revision.load(Ordering::SeqCst) != revision {
            return Ok(peritus_agent::DeveloperRequestAdmission::Stale);
        }
        self.applied.lock().expect("applied").push(revision);
        Ok(peritus_agent::DeveloperRequestAdmission::Accepted)
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

struct AdmissionRace {
    revision: AtomicU64,
    observed: Mutex<Vec<(u64, peritus_model_protocol::RequestFingerprint)>>,
    fail: bool,
}
impl DeveloperInteraction for AdmissionRace {
    fn input(&self) -> Result<DeveloperInput, DeveloperLoopError> {
        let revision = self.revision.load(Ordering::SeqCst);
        Ok(DeveloperInput {
            revision,
            conversation: format!("governing input revision {revision}"),
            images: Vec::new(),
        })
    }
    fn prepare_request(
        &self,
        revision: u64,
        request: &ModelRequest,
    ) -> Result<peritus_agent::DeveloperRequestAdmission, DeveloperLoopError> {
        self.observed.lock().expect("observed").push((revision, request.fingerprint()?));
        if self.fail {
            return Err(DeveloperLoopError::Trace("durable admission failed".to_owned()));
        }
        if self.revision.compare_exchange(1, 2, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            return Ok(peritus_agent::DeveloperRequestAdmission::Stale);
        }
        assert_eq!(revision, 2);
        Ok(peritus_agent::DeveloperRequestAdmission::Accepted)
    }
    fn observe(&self, _: DeveloperActivity<'_>) -> Result<(), DeveloperLoopError> {
        Ok(())
    }
}

#[test]
fn exact_prepared_request_is_checked_before_send_and_stale_context_is_rebuilt() {
    block_on(async {
        for fail in [false, true] {
            let provider = ScriptedProvider {
                profile: parallel_profile(),
                responses: Mutex::new(VecDeque::from([text_response()])),
                requests: Mutex::new(Vec::new()),
            };
            let port = AdmissionRace {
                revision: AtomicU64::new(1),
                observed: Mutex::new(Vec::new()),
                fail,
            };
            let outcome = DeveloperLoop::run_interactive(
                &provider,
                request(CancellationToken::new()),
                &mut RecordingTool::default(),
                &mut RecordingTrace::default(),
                None,
                &port,
            )
            .await;
            let sent = provider.requests.lock().expect("sent").clone();
            let prepared = port.observed.lock().expect("prepared").clone();
            if fail {
                assert!(outcome.is_err());
                assert!(sent.is_empty(), "admission failure must not consume a provider request");
                assert_eq!(prepared.len(), 1);
            } else {
                outcome.expect("rebuilt request succeeds");
                assert_eq!(prepared.len(), 2);
                assert_eq!(sent.len(), 1, "stale prepared request must not be sent");
                assert_eq!(prepared[1], (2, sent[0].fingerprint().expect("exact fingerprint")));
                assert_ne!(prepared[0].1, prepared[1].1);
                let visible = sent[0]
                    .messages()
                    .iter()
                    .flat_map(Message::content)
                    .filter_map(|block| {
                        if let ContentBlock::Text(text) = block {
                            Some(text.expose_for_wire())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    !visible.contains("governing input revision 1"),
                    "a stale prepared user view must not survive in the rebuilt request"
                );
                assert_eq!(visible.matches("Current governing conversation").count(), 1);
            }
        }
    });
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
        let mut trace = RecordingTrace::default();
        let result = DeveloperLoop::run_interactive(
            &provider,
            request(cancellation),
            &mut tools,
            &mut trace,
            None,
            &port,
        )
        .await;
        assert!(matches!(result, Err(DeveloperLoopError::Cancelled)));
        assert_eq!(tools.calls, 1);
        assert_eq!(
            trace
                .accounting
                .iter()
                .filter(|event| matches!(event, peritus_agent::DeveloperAccountingEvent::ToolCall))
                .count(),
            1,
            "a completed effect remains counted when the next call is cancelled"
        );
        assert_eq!(
            trace
                .accounting
                .iter()
                .filter(|event| matches!(
                    event,
                    peritus_agent::DeveloperAccountingEvent::ModelRequest { .. }
                ))
                .count(),
            1
        );
        assert_eq!(provider.requests.lock().expect("requests").len(), 1);
    });
}

#[derive(Default)]
struct HostHandoff {
    calls: u32,
}

impl DeveloperToolExecutor for HostHandoff {
    fn execute(
        &mut self,
        _: &CompletedToolCall,
    ) -> Result<DeveloperToolObservation, DeveloperLoopError> {
        self.calls += 1;
        Ok(DeveloperToolObservation {
            output: CanonicalJson::parse("{}", JsonBounds::value(ProtocolLimits::PRODUCTION))?,
            is_error: false,
        })
    }

    fn yields_to_host(&self) -> bool {
        self.calls > 0
    }
}

#[test]
fn host_handoff_stops_the_batch_and_returns_without_another_model_request() {
    block_on(async {
        let provider = ScriptedProvider {
            profile: parallel_profile(),
            responses: Mutex::new(VecDeque::from([batch_tool_response()])),
            requests: Mutex::new(Vec::new()),
        };
        let mut tools = HostHandoff::default();
        let result = DeveloperLoop::run(
            &provider,
            request(CancellationToken::new()),
            &mut tools,
            &mut RecordingTrace::default(),
        )
        .await
        .expect("host handoff");
        assert_eq!(tools.calls, 1);
        assert_eq!(result.model_turns, 1);
        assert!(result.text.is_empty());
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
