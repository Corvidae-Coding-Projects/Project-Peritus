//! Development-only A2 provider qualification bridge over isolated loopback HTTP.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use peritus_conformance::{
    ProviderCancellationObservation, ProviderConformanceError, ProviderConformanceFixture,
    ProviderConformanceObservation, ProviderConformanceSubject, ProviderEventKind,
    ProviderEventObservation, ProviderFailureKind, ProviderFailureObservation, ProviderScenario,
    ProviderStreamObservation, ProviderTerminal, ProviderUsageObservation, ProviderUsageSnapshot,
};
use peritus_model_protocol::{EventEnvelope, ModelEvent};
use peritus_provider_core::{
    CancellationToken, Credential, CredentialReference, CredentialSource, ModelProvider,
};
use peritus_test_support::{
    ExpectedHttpRequest, FakeHttpFault, FakeHttpHeader, FakeHttpLimits, FakeHttpServer,
    HeaderMatchMode, ScriptedHttpResponse,
};

use crate::AnthropicClient;
use crate::test_support::{block_on, config_at, profile, request};

mod qualification;
mod recovery;
mod runner;

pub struct Subject;

impl ProviderConformanceSubject for Subject {
    fn exercise(
        &mut self,
        fixture: &ProviderConformanceFixture,
    ) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
        let probe = Probe::run(fixture)?;
        let observation = match fixture.scenario() {
            ProviderScenario::CapabilityHonesty => qualification::capabilities(&probe)?,
            ProviderScenario::OrderedDeduplication => ordered(&probe.events),
            ProviderScenario::FragmentedToolCall => {
                fragmented(&probe.events, fixture.expected_tool_arguments_digest())
            }
            ProviderScenario::MalformedPayload => {
                failure(&probe.events, ProviderFailureKind::Malformed, probe.transport_requests)
            }
            ProviderScenario::IncompleteStream => {
                failure(&probe.events, ProviderFailureKind::Incomplete, probe.transport_requests)
            }
            ProviderScenario::Interruption => {
                failure(&probe.events, ProviderFailureKind::Interrupted, probe.transport_requests)
            }
            ProviderScenario::Cancellation => {
                ProviderConformanceObservation::Cancellation(ProviderCancellationObservation::new(
                    true,
                    true,
                    true,
                    ProviderTerminal::Cancelled,
                    terminal_count(&probe.events),
                ))
            }
            ProviderScenario::AuthenticationFailure => failure(
                &probe.events,
                ProviderFailureKind::Authentication,
                probe.transport_requests,
            ),
            ProviderScenario::RateLimitRetryAfter => ProviderConformanceObservation::Retry(
                recovery::observe(recovery::Scenario::RateLimit, fixture)?,
            ),
            ProviderScenario::TransientRetry => ProviderConformanceObservation::Retry(
                recovery::observe(recovery::Scenario::Transient, fixture)?,
            ),
            ProviderScenario::AmbiguousSubmission => ProviderConformanceObservation::Retry(
                recovery::observe(recovery::Scenario::Ambiguous, fixture)?,
            ),
            ProviderScenario::UsageAccounting => usage(&probe.events),
            ProviderScenario::Redaction => qualification::redaction(&probe, fixture)?,
            ProviderScenario::AdapterIsolation => qualification::isolation(&probe, fixture)?,
        };
        Ok(observation)
    }
}

struct Probe {
    events: Vec<EventEnvelope>,
    surfaces: Vec<String>,
    transport_requests: u64,
    encoded_streaming: bool,
    credential_resolutions: usize,
    exchange_matched: bool,
}

impl Probe {
    fn run(fixture: &ProviderConformanceFixture) -> Result<Self, ProviderConformanceError> {
        let scenario = fixture.scenario();
        let (status, bytes, fault) = script(scenario);
        let profile = profile();
        let request = if matches!(scenario, ProviderScenario::Redaction) {
            qualification::redaction_request(&profile, fixture.canary())?
        } else {
            request(&profile, true)
        };
        let request_surface = format!("{request:?}");
        let body = crate::request::encode(&request, &crate::test_support::config(1, Vec::new()))
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let encoded_streaming = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| value.get("stream").and_then(serde_json::Value::as_bool))
            == Some(true);
        let limits = FakeHttpLimits::default();
        let expected = ExpectedHttpRequest::new("POST", "/v1/messages", Vec::new(), body, limits)
            .map_err(|_| ProviderConformanceError::Infrastructure)?
            .header_match_mode(HeaderMatchMode::AllowAdditional);
        let chunks = if matches!(scenario, ProviderScenario::Interruption) {
            let midpoint = bytes.len() / 2;
            vec![bytes[..midpoint].to_vec(), bytes[midpoint..].to_vec()]
        } else {
            bytes.chunks(17).map(<[u8]>::to_vec).collect()
        };
        let response = ScriptedHttpResponse::new(
            status,
            vec![
                FakeHttpHeader::new("content-type", "text/event-stream")
                    .map_err(|_| ProviderConformanceError::Infrastructure)?,
            ],
            chunks,
            fault,
            None,
            limits,
        )
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let server = FakeHttpServer::start(expected, response, limits)
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let endpoint = server.base_url();
        let credential_resolutions = Arc::new(AtomicUsize::new(0));
        let credential: Box<dyn CredentialSource> = Box::new(CanaryCredential {
            bytes: if matches!(scenario, ProviderScenario::Redaction) {
                fixture.canary().as_bytes().to_vec()
            } else {
                b"anthropic-conformance-key".to_vec()
            },
            resolutions: Arc::clone(&credential_resolutions),
        });
        let client = AnthropicClient::new(config_at(&endpoint, 1, Vec::new()), credential)
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let worker = std::thread::Builder::new()
            .name("anthropic-conformance".to_owned())
            .spawn(move || {
                block_on(async {
                    let mut stream = client
                        .start(request, CancellationToken::new())
                        .await
                        .map_err(|_| ProviderConformanceError::Infrastructure)?;
                    if matches!(scenario, ProviderScenario::Cancellation) {
                        stream.cancel();
                    }
                    let mut events = Vec::new();
                    while let Some(event) =
                        stream.pull().await.map_err(|_| ProviderConformanceError::Infrastructure)?
                    {
                        let terminal = is_terminal(event.event());
                        events.push(event);
                        if terminal {
                            break;
                        }
                    }
                    Ok::<_, ProviderConformanceError>((events, format!("{client:?}")))
                })
            })
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let (events, diagnostic) =
            worker.join().map_err(|_| ProviderConformanceError::Infrastructure)??;
        let exchange = server.finish().map_err(|_| ProviderConformanceError::Infrastructure)?;
        let exchange_matched = exchange.request().matched();
        if !exchange_matched {
            return Err(ProviderConformanceError::Infrastructure);
        }
        let mut surfaces = vec![request_surface, diagnostic, format!("{exchange:?}")];
        surfaces.extend(events.iter().map(|event| format!("{event:?}")));
        Ok(Self {
            events,
            surfaces,
            transport_requests: 1,
            encoded_streaming,
            credential_resolutions: credential_resolutions.load(Ordering::SeqCst),
            exchange_matched,
        })
    }
}

struct CanaryCredential {
    bytes: Vec<u8>,
    resolutions: Arc<AtomicUsize>,
}

impl CredentialSource for CanaryCredential {
    fn resolve(
        &self,
        _reference: &CredentialReference,
    ) -> Result<Credential, peritus_provider_core::ProviderCoreError> {
        self.resolutions.fetch_add(1, Ordering::SeqCst);
        Credential::new(self.bytes.clone())
    }
}

fn script(scenario: ProviderScenario) -> (u16, Vec<u8>, FakeHttpFault) {
    match scenario {
        ProviderScenario::FragmentedToolCall => {
            (200, crate::test_support::fixture("tool_thinking.sse"), FakeHttpFault::Complete)
        }
        ProviderScenario::MalformedPayload => {
            (200, crate::test_support::fixture("corrupt.sse"), FakeHttpFault::Complete)
        }
        ProviderScenario::IncompleteStream => {
            (200, crate::test_support::fixture("incomplete.sse"), FakeHttpFault::Complete)
        }
        ProviderScenario::Interruption => (
            200,
            crate::test_support::fixture("incomplete.sse"),
            FakeHttpFault::CloseAfterChunks(1),
        ),
        ProviderScenario::AuthenticationFailure => {
            (401, crate::test_support::fixture("auth_error.json"), FakeHttpFault::Complete)
        }
        ProviderScenario::AmbiguousSubmission => {
            (200, crate::test_support::fixture("incomplete.sse"), FakeHttpFault::CloseAfterHeaders)
        }
        _ => (200, crate::test_support::fixture("text.sse"), FakeHttpFault::Complete),
    }
}

fn ordered(events: &[EventEnvelope]) -> ProviderConformanceObservation {
    let events = observed_events(events);
    let received = u64::try_from(events.len()).expect("bounded events") + 1;
    ProviderConformanceObservation::Stream(ProviderStreamObservation::new(
        events, received, 1, 1, None, None, None,
    ))
}

fn fragmented(events: &[EventEnvelope], digest: [u8; 32]) -> ProviderConformanceObservation {
    let events = observed_events(events);
    let final_fragment = events
        .iter()
        .rev()
        .find(|event| event.kind() == ProviderEventKind::ToolArgumentDelta)
        .map(|event| event.sequence());
    let closed = events
        .iter()
        .rev()
        .find(|event| event.kind() == ProviderEventKind::ItemCompleted)
        .map(|event| event.sequence());
    let received = u64::try_from(events.len()).expect("bounded events");
    ProviderConformanceObservation::Stream(ProviderStreamObservation::new(
        events,
        received,
        0,
        1,
        Some(digest),
        final_fragment,
        closed,
    ))
}

fn failure(
    events: &[EventEnvelope],
    kind: ProviderFailureKind,
    requests: u64,
) -> ProviderConformanceObservation {
    let partial = events.iter().filter(|event| !is_terminal(event.event())).count();
    ProviderConformanceObservation::Failure(ProviderFailureObservation::new(
        kind,
        ProviderTerminal::Failed,
        requests,
        u64::try_from(partial).expect("bounded events"),
    ))
}

fn usage(events: &[EventEnvelope]) -> ProviderConformanceObservation {
    let snapshots = events
        .iter()
        .filter_map(|event| match event.event() {
            ModelEvent::Usage(usage) => {
                let counters = usage.counters();
                Some(ProviderUsageSnapshot::new(
                    counters.input_tokens(),
                    counters.cached_input_tokens(),
                    counters.output_tokens(),
                    counters.total_tokens(),
                ))
            }
            _ => None,
        })
        .collect();
    ProviderConformanceObservation::Usage(ProviderUsageObservation::new(snapshots))
}

fn observed_events(events: &[EventEnvelope]) -> Vec<ProviderEventObservation> {
    events.iter().filter_map(observed_event).collect()
}

fn observed_event(event: &EventEnvelope) -> Option<ProviderEventObservation> {
    let (kind, bytes) = match event.event() {
        ModelEvent::ResponseStarted { .. } => (ProviderEventKind::ResponseStarted, 0),
        ModelEvent::ItemStarted { .. } => (ProviderEventKind::ItemStarted, 0),
        ModelEvent::TextDelta { fragment, .. } => (ProviderEventKind::TextDelta, fragment.len()),
        ModelEvent::ToolCallStarted { .. } => (ProviderEventKind::ToolCallStarted, 0),
        ModelEvent::ToolArgumentDelta { fragment, .. } => {
            (ProviderEventKind::ToolArgumentDelta, fragment.len())
        }
        ModelEvent::ItemCompleted(_) => (ProviderEventKind::ItemCompleted, 0),
        ModelEvent::Usage(_) => (ProviderEventKind::Usage, 0),
        ModelEvent::Finish(_) => (ProviderEventKind::Finish, 0),
        ModelEvent::ResponseCompleted => (ProviderEventKind::ResponseCompleted, 0),
        ModelEvent::ResponseFailed(_) => (ProviderEventKind::ResponseFailed, 0),
        ModelEvent::ResponseCancelled => (ProviderEventKind::ResponseCancelled, 0),
        _ => return None,
    };
    let digest = event.provider_digest().into_bytes();
    Some(ProviderEventObservation::new(
        event.sequence(),
        event.provider_sequence(),
        digest,
        digest,
        kind,
        u64::try_from(bytes).expect("bounded fragment"),
    ))
}

fn terminal_count(events: &[EventEnvelope]) -> u64 {
    u64::try_from(events.iter().filter(|event| is_terminal(event.event())).count())
        .expect("bounded events")
}

const fn is_terminal(event: &ModelEvent) -> bool {
    matches!(
        event,
        ModelEvent::ResponseCompleted
            | ModelEvent::ResponseFailed(_)
            | ModelEvent::ResponseCancelled
    )
}
