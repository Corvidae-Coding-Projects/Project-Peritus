//! Development-only A2 bridge through production request/client/stream paths.

mod observations;
mod qualification;
mod recovery;
mod runner;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use peritus_conformance::{
    ProviderCancellationObservation, ProviderConformanceError, ProviderConformanceFixture,
    ProviderConformanceObservation, ProviderConformanceSubject, ProviderFailureKind,
    ProviderScenario, ProviderTerminal,
};
use peritus_model_protocol::{EventEnvelope, FailureCategory, WireDialect};
use peritus_provider_core::{
    CancellationToken, Credential, CredentialReference, CredentialSource, ModelProvider,
};
use peritus_test_support::{
    ExpectedHttpRequest, FakeHttpFault, FakeHttpHeader, FakeHttpLimits, FakeHttpReleasePoint,
    FakeHttpServer, HeaderMatchMode, ScriptedHttpResponse,
};

use crate::GoogleClient;
use crate::test_support::{
    block_on, config_at_with_profile, fixture, profile, request, streaming_profile,
};

pub struct Subject;

impl ProviderConformanceSubject for Subject {
    fn exercise(
        &mut self,
        fixture: &ProviderConformanceFixture,
    ) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
        match fixture.scenario() {
            ProviderScenario::RateLimitRetryAfter => recovery::observe_rate(fixture),
            ProviderScenario::TransientRetry => recovery::observe_transient(fixture),
            ProviderScenario::AmbiguousSubmission => recovery::observe_ambiguous(),
            scenario => {
                let probe = Probe::run(fixture)?;
                Ok(match scenario {
                    ProviderScenario::CapabilityHonesty => qualification::capabilities(&probe)?,
                    ProviderScenario::OrderedDeduplication => observations::ordered(&probe),
                    ProviderScenario::FragmentedToolCall => {
                        observations::fragmented(&probe, fixture.expected_tool_arguments_digest())?
                    }
                    ProviderScenario::MalformedPayload => observations::failure(
                        &probe,
                        ProviderFailureKind::Malformed,
                        FailureCategory::MalformedPayload,
                    )?,
                    ProviderScenario::IncompleteStream => observations::failure(
                        &probe,
                        ProviderFailureKind::Incomplete,
                        FailureCategory::IncompleteStream,
                    )?,
                    ProviderScenario::Interruption => observations::failure(
                        &probe,
                        ProviderFailureKind::Interrupted,
                        FailureCategory::Transport,
                    )?,
                    ProviderScenario::Cancellation => ProviderConformanceObservation::Cancellation(
                        ProviderCancellationObservation::new(
                            probe.cancellation_observed.observed(),
                            probe.release_observed.observed(),
                            probe.worker_joined.observed(),
                            ProviderTerminal::Cancelled,
                            observations::terminal_count(&probe.events),
                        ),
                    ),
                    ProviderScenario::AuthenticationFailure => observations::failure(
                        &probe,
                        ProviderFailureKind::Authentication,
                        FailureCategory::Authentication,
                    )?,
                    ProviderScenario::UsageAccounting => observations::usage(&probe.events),
                    ProviderScenario::Redaction => qualification::redaction(&probe, fixture)?,
                    ProviderScenario::AdapterIsolation => {
                        qualification::isolation(&probe, fixture)?
                    }
                    ProviderScenario::RateLimitRetryAfter
                    | ProviderScenario::TransientRetry
                    | ProviderScenario::AmbiguousSubmission => {
                        return Err(ProviderConformanceError::Infrastructure);
                    }
                })
            }
        }
    }
}

pub struct Probe {
    events: Vec<EventEnvelope>,
    surfaces: Vec<String>,
    transport_requests: u64,
    encoded_streaming: Evidence,
    credential_resolutions: usize,
    exchange_matched: Evidence,
    duplicate_events: u64,
    cancellation_observed: Evidence,
    release_observed: Evidence,
    worker_joined: Evidence,
    sensitive_inputs: u64,
    configured_google: Evidence,
    request_google: Evidence,
}

impl Probe {
    #[allow(clippy::too_many_lines, reason = "owns one complete isolated HTTP probe lifecycle")]
    fn run(case: &ProviderConformanceFixture) -> Result<Self, ProviderConformanceError> {
        let scenario = case.scenario();
        let (status, bytes, fault) = script(scenario, case.canary());
        let profile = if scenario == ProviderScenario::CapabilityHonesty {
            streaming_profile(WireDialect::GeminiInteractionsV1)
        } else {
            profile(WireDialect::GeminiInteractionsV1)
        };
        let (request, request_sensitive_inputs) = if scenario == ProviderScenario::Redaction {
            qualification::redaction_request(&profile, case.canary())?
        } else {
            (request(&profile, true), 0)
        };
        let configured_google = profile.provider().as_str() == "google";
        let request_google = request.profile_id() == profile.profile_id()
            && request.dialect() == WireDialect::GeminiInteractionsV1;
        let request_surface = format!("{request:?}");
        let encoded = crate::request::encode(
            &request,
            crate::test_support::config(WireDialect::GeminiInteractionsV1, 1).endpoint(),
        )
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let encoded_streaming = serde_json::from_slice::<serde_json::Value>(&encoded.body)
            .ok()
            .and_then(|value| value.get("stream").and_then(serde_json::Value::as_bool))
            == Some(true);
        let limits = FakeHttpLimits::default();
        let expected =
            ExpectedHttpRequest::new("POST", "/v1/interactions", Vec::new(), encoded.body, limits)
                .map_err(|_| ProviderConformanceError::Infrastructure)?
                .header_match_mode(HeaderMatchMode::AllowAdditional);
        let chunks = response_chunks(scenario, &bytes);
        let release = match scenario {
            ProviderScenario::Cancellation => Some(FakeHttpReleasePoint::BeforeChunk(0)),
            ProviderScenario::Interruption => Some(FakeHttpReleasePoint::BeforeClose),
            _ => None,
        };
        let mut headers = (status == 200)
            .then(|| header("content-type", "text/event-stream"))
            .transpose()?
            .into_iter()
            .collect::<Vec<_>>();
        if scenario == ProviderScenario::Interruption {
            headers.push(header("content-length", &bytes.len().to_string())?);
        }
        let response = ScriptedHttpResponse::new(status, headers, chunks, fault, release, limits)
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let server = FakeHttpServer::start(expected, response, limits)
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let endpoint = server.base_url();
        let resolutions = Arc::new(AtomicUsize::new(0));
        let credential_bytes = if scenario == ProviderScenario::Redaction {
            case.canary().as_bytes().to_vec()
        } else {
            b"google-conformance-key".to_vec()
        };
        let credential_sensitive = contains_bytes(&credential_bytes, case.canary().as_bytes());
        let response_sensitive = contains_bytes(&bytes, case.canary().as_bytes());
        let sensitive_inputs = request_sensitive_inputs
            .checked_add(u64::from(credential_sensitive))
            .and_then(|count| count.checked_add(u64::from(response_sensitive)))
            .ok_or(ProviderConformanceError::Infrastructure)?;
        let credential =
            CanaryCredential { bytes: credential_bytes, resolutions: Arc::clone(&resolutions) };
        let client =
            GoogleClient::new(config_at_with_profile(&endpoint, profile, 1), Box::new(credential))
                .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let worker = std::thread::Builder::new()
            .name("google-conformance".to_owned())
            .spawn(move || run_client(&client, request, scenario))
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let controlled =
            matches!(scenario, ProviderScenario::Cancellation | ProviderScenario::Interruption);
        let release_observed = if controlled {
            server
                .wait_until_blocked(Duration::from_secs(5))
                .map_err(|_| ProviderConformanceError::Infrastructure)?;
            true
        } else {
            false
        };
        if scenario == ProviderScenario::Interruption {
            server.release().map_err(|_| ProviderConformanceError::Infrastructure)?;
        }
        let (events, diagnostic, cancellation_observed) =
            worker.join().map_err(|_| ProviderConformanceError::Infrastructure)??;
        if scenario == ProviderScenario::Cancellation {
            server.release().map_err(|_| ProviderConformanceError::Infrastructure)?;
        }
        let exchange = server.finish().map_err(|_| ProviderConformanceError::Infrastructure)?;
        if !exchange.request().matched() {
            return Err(ProviderConformanceError::Infrastructure);
        }
        let duplicate_events = duplicate_ids(&bytes);
        let mut surfaces = vec![request_surface, diagnostic, format!("{exchange:?}")];
        surfaces.extend(events.iter().map(|event| format!("{event:?}")));
        Ok(Self {
            events,
            surfaces,
            transport_requests: 1,
            encoded_streaming: Evidence::from(encoded_streaming),
            credential_resolutions: resolutions.load(Ordering::SeqCst),
            exchange_matched: Evidence::Observed,
            duplicate_events,
            cancellation_observed: Evidence::from(cancellation_observed),
            release_observed: Evidence::from(release_observed),
            worker_joined: Evidence::Observed,
            sensitive_inputs,
            configured_google: Evidence::from(configured_google),
            request_google: Evidence::from(request_google),
        })
    }
}

fn run_client(
    client: &GoogleClient,
    request: peritus_model_protocol::ModelRequest,
    scenario: ProviderScenario,
) -> Result<(Vec<EventEnvelope>, String, bool), ProviderConformanceError> {
    block_on(async {
        let mut stream = client
            .start(request, CancellationToken::new())
            .await
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let cancelled = if scenario == ProviderScenario::Cancellation {
            stream.cancel();
            stream.cancellation_token().is_cancelled()
        } else {
            false
        };
        let mut events = Vec::new();
        while let Some(event) =
            stream.pull().await.map_err(|_| ProviderConformanceError::Infrastructure)?
        {
            let terminal = observations::is_terminal(event.event());
            events.push(event);
            if terminal {
                break;
            }
        }
        Ok((events, format!("{client:?}"), cancelled))
    })
}

pub struct CanaryCredential {
    bytes: Vec<u8>,
    resolutions: Arc<AtomicUsize>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Evidence {
    Observed,
    Absent,
}

impl Evidence {
    pub(super) const fn observed(self) -> bool {
        matches!(self, Self::Observed)
    }
}

impl From<bool> for Evidence {
    fn from(value: bool) -> Self {
        if value { Self::Observed } else { Self::Absent }
    }
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

fn script(scenario: ProviderScenario, canary: &str) -> (u16, Vec<u8>, FakeHttpFault) {
    match scenario {
        ProviderScenario::OrderedDeduplication => {
            (200, fixture("interactions_ordered_duplicate.sse"), FakeHttpFault::Complete)
        }
        ProviderScenario::FragmentedToolCall => {
            (200, fixture("interactions_tool_thinking.sse"), FakeHttpFault::Complete)
        }
        ProviderScenario::MalformedPayload => {
            (200, fixture("corrupt.sse"), FakeHttpFault::Complete)
        }
        ProviderScenario::IncompleteStream => {
            (200, fixture("incomplete.sse"), FakeHttpFault::Complete)
        }
        ProviderScenario::Interruption => {
            (200, fixture("incomplete.sse"), FakeHttpFault::CloseAfterChunks(1))
        }
        ProviderScenario::Cancellation => {
            (200, fixture("interactions_success.sse"), FakeHttpFault::Complete)
        }
        ProviderScenario::AuthenticationFailure => {
            (401, fixture("auth_error.json"), FakeHttpFault::Complete)
        }
        ProviderScenario::Redaction => (200, redaction_stream(canary), FakeHttpFault::Complete),
        _ => (200, fixture("interactions_success.sse"), FakeHttpFault::Complete),
    }
}

fn response_chunks(scenario: ProviderScenario, bytes: &[u8]) -> Vec<Vec<u8>> {
    if scenario == ProviderScenario::Interruption {
        let marker = b"id: i-3";
        let split =
            bytes.windows(marker.len()).position(|window| window == marker).unwrap_or(bytes.len());
        return vec![bytes[..split].to_vec(), bytes[split..].to_vec()];
    }
    bytes.chunks(17).map(<[u8]>::to_vec).collect()
}

fn redaction_stream(canary: &str) -> Vec<u8> {
    format!("event: interaction.created\ndata: {{\"event_type\":\"interaction.created\",\"interaction\":{{\"id\":\"int_redacted\",\"model\":\"gemini-3.7-flash\"}}}}\n\nevent: error\ndata: {{\"event_type\":\"error\",\"error\":{{\"code\":\"api_error\",\"message\":\"{canary}\"}}}}\n\n").into_bytes()
}

fn header(name: &str, value: &str) -> Result<FakeHttpHeader, ProviderConformanceError> {
    FakeHttpHeader::new(name, value).map_err(|_| ProviderConformanceError::Infrastructure)
}

fn duplicate_ids(bytes: &[u8]) -> u64 {
    let text = core::str::from_utf8(bytes).unwrap_or_default();
    let mut seen = std::collections::BTreeSet::new();
    u64::try_from(
        text.lines()
            .filter_map(|line| line.strip_prefix("id: "))
            .filter(|id| !seen.insert((*id).to_owned()))
            .count(),
    )
    .unwrap_or(0)
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|window| window == needle)
}
