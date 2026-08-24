//! Development-only A2 qualification bridge over the production adapter and fresh fake servers.

mod observations;
mod recovery;
mod redaction;
mod runner;

use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use peritus_conformance::{
    ProviderCancellationObservation, ProviderConformanceError, ProviderConformanceFixture,
    ProviderConformanceObservation, ProviderConformanceSubject, ProviderFailureKind,
    ProviderScenario, ProviderTerminal,
};
use peritus_model_protocol::EventEnvelope;
use peritus_provider_core::{
    BoxFuture, CancellationToken, Credential, CredentialReference, CredentialSource, Endpoint,
    HttpRequest, HttpTransport, ModelProvider, ProviderCoreError, ReqwestTransport,
};
use peritus_test_support::{
    ExpectedHttpRequest, FakeHttpFault, FakeHttpHeader, FakeHttpLimits, FakeHttpServer,
    HeaderMatchMode, ScriptedHttpResponse,
};

use super::support::{
    block_on, credential_reference, fixture as fixture_bytes, minimal_request, profile_full,
    profile_minimal, profile_streaming_only, redaction_request, request_with_id,
};
use crate::{OpenAiConfig, OpenAiProvider};

pub(super) struct Subject;

impl ProviderConformanceSubject for Subject {
    fn exercise(
        &mut self,
        fixture: &ProviderConformanceFixture,
    ) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
        let probe = Probe::run(fixture)?;
        let observation = match fixture.scenario() {
            ProviderScenario::CapabilityHonesty => observations::capabilities(&probe)?,
            ProviderScenario::OrderedDeduplication => observations::ordered(&probe.events),
            ProviderScenario::FragmentedToolCall => {
                observations::fragmented(&probe.events, fixture.expected_tool_arguments_digest())
            }
            ProviderScenario::MalformedPayload => observations::failure(
                &probe.events,
                ProviderFailureKind::Malformed,
                probe.transport_requests,
            ),
            ProviderScenario::IncompleteStream => observations::failure(
                &probe.events,
                ProviderFailureKind::Incomplete,
                probe.transport_requests,
            ),
            ProviderScenario::Interruption => observations::failure(
                &probe.events,
                ProviderFailureKind::Interrupted,
                probe.transport_requests,
            ),
            ProviderScenario::Cancellation => {
                ProviderConformanceObservation::Cancellation(ProviderCancellationObservation::new(
                    true,
                    true,
                    true,
                    ProviderTerminal::Cancelled,
                    observations::terminal_count(&probe.events),
                ))
            }
            ProviderScenario::AuthenticationFailure => observations::failure(
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
            ProviderScenario::UsageAccounting => observations::usage(&probe.events),
            ProviderScenario::Redaction => redaction::observe(&probe, fixture)?,
            ProviderScenario::AdapterIsolation => observations::isolation(&probe, fixture)?,
        };
        Ok(observation)
    }
}

struct Probe {
    events: Vec<EventEnvelope>,
    surfaces: Vec<String>,
    transport_requests: u64,
    credential_resolutions: usize,
    facts: ProbeFacts,
    credential_adapter: Option<String>,
    transport_adapter: Option<String>,
    sensitive_inputs: usize,
}

impl Probe {
    #[allow(
        clippy::too_many_lines,
        reason = "the bridge keeps one fake-server request, production stream, and observations together"
    )]
    fn run(fixture: &ProviderConformanceFixture) -> Result<Self, ProviderConformanceError> {
        let scenario = fixture.scenario();
        let (status, bytes, fault) = script(scenario);
        let profile = match scenario {
            ProviderScenario::CapabilityHonesty => profile_streaming_only(),
            ProviderScenario::Redaction => profile_full(),
            _ => profile_minimal(),
        };
        let request = match scenario {
            ProviderScenario::Redaction => redaction_request(&profile, fixture.canary()),
            ProviderScenario::AdapterIsolation => {
                request_with_id(&profile, fixture.selected_adapter())
            }
            _ => minimal_request(&profile),
        };
        let request_bound_selected =
            request.request_id().expose_for_wire() == fixture.selected_adapter();
        let request_sensitive_inputs = if scenario == ProviderScenario::Redaction {
            redaction::request_canary_count(&request, fixture.canary())
        } else {
            0
        };
        let request_surface = format!("{request:?}");
        let body = crate::request::encode(&request)
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let encoded_streaming = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| value.get("stream").and_then(serde_json::Value::as_bool))
            == Some(true);
        let limits = FakeHttpLimits::default();
        let expected = ExpectedHttpRequest::new("POST", "/v1/responses", Vec::new(), body, limits)
            .map_err(|_| ProviderConformanceError::Infrastructure)?
            .header_match_mode(HeaderMatchMode::AllowAdditional);
        let chunks = if scenario == ProviderScenario::Interruption {
            let midpoint = bytes.len() / 2;
            vec![bytes[..midpoint].to_vec(), bytes[midpoint..].to_vec()]
        } else {
            bytes.chunks(23).map(<[u8]>::to_vec).collect()
        };
        let mut response_headers = vec![
            FakeHttpHeader::new("content-type", "text/event-stream")
                .map_err(|_| ProviderConformanceError::Infrastructure)?,
        ];
        let response_canary_injected = scenario == ProviderScenario::Redaction;
        if response_canary_injected {
            response_headers.push(
                FakeHttpHeader::new("set-cookie", fixture.canary())
                    .map_err(|_| ProviderConformanceError::Infrastructure)?,
            );
        }
        let response =
            ScriptedHttpResponse::new(status, response_headers, chunks, fault, None, limits)
                .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let server = FakeHttpServer::start(expected, response, limits)
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let endpoint = Endpoint::new(server.base_url())
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let config = OpenAiConfig::for_test(endpoint, credential_reference())
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let configured_selected = config.endpoint().as_str().trim_end_matches('/')
            == server.base_url().trim_end_matches('/');
        let config_surface = format!("{config:?}");
        let secret = if scenario == ProviderScenario::Redaction {
            fixture.canary().as_bytes()
        } else {
            b"openai-conformance-key"
        };
        let credential_canary_injected = secret == fixture.canary().as_bytes();
        let credential_resolutions = Arc::new(AtomicUsize::new(0));
        let adapter_label = if scenario == ProviderScenario::AdapterIsolation {
            fixture.selected_adapter()
        } else {
            "openai-responses"
        };
        let credential_observations = Arc::new(Mutex::new(Vec::new()));
        let transport_observations = Arc::new(Mutex::new(Vec::new()));
        let transport = ReqwestTransport::new(config.http_limits())
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let provider = OpenAiProvider::with_transport(
            config,
            profile,
            Arc::new(CanaryCredential {
                bytes: secret.to_vec(),
                resolutions: Arc::clone(&credential_resolutions),
                adapter: adapter_label.to_owned(),
                observations: Arc::clone(&credential_observations),
            }),
            Arc::new(TaggedTransport {
                inner: transport,
                adapter: adapter_label.to_owned(),
                observations: Arc::clone(&transport_observations),
            }),
        )
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let worker = std::thread::Builder::new()
            .name("openai-conformance".to_owned())
            .spawn(move || {
                block_on(async {
                    let mut stream = provider
                        .start(request, CancellationToken::new())
                        .await
                        .map_err(|_| ProviderConformanceError::Infrastructure)?;
                    if scenario == ProviderScenario::Cancellation {
                        stream.cancel();
                    }
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
                    Ok::<_, ProviderConformanceError>((events, format!("{provider:?}")))
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
        let mut surfaces =
            vec![request_surface, config_surface, diagnostic, format!("{exchange:?}")];
        surfaces.extend(events.iter().map(|event| format!("{event:?}")));
        let routing_canary_injected = if scenario == ProviderScenario::Redaction {
            let routing_error = OpenAiConfig::new(credential_reference())
                .and_then(|value| value.with_organization(fixture.canary().to_owned()))
                .expect_err("canary routing identity is invalid");
            surfaces.push(format!("{routing_error:?}"));
            true
        } else {
            false
        };
        let sensitive_inputs = request_sensitive_inputs
            + usize::from(credential_canary_injected)
            + usize::from(response_canary_injected)
            + usize::from(routing_canary_injected);
        let credential_adapter = credential_observations
            .lock()
            .map_err(|_| ProviderConformanceError::Infrastructure)?
            .first()
            .cloned();
        let transport_adapter = transport_observations
            .lock()
            .map_err(|_| ProviderConformanceError::Infrastructure)?
            .first()
            .cloned();
        let transport_requests = u64::try_from(
            transport_observations
                .lock()
                .map_err(|_| ProviderConformanceError::Infrastructure)?
                .len(),
        )
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
        Ok(Self {
            events,
            surfaces,
            transport_requests,
            credential_resolutions: credential_resolutions.load(Ordering::SeqCst),
            facts: ProbeFacts::new(&[
                (ProbeFacts::ENCODED_STREAMING, encoded_streaming),
                (ProbeFacts::EXCHANGE_MATCHED, exchange_matched),
                (ProbeFacts::CONFIGURED_SELECTED, configured_selected),
                (ProbeFacts::REQUEST_BOUND_SELECTED, request_bound_selected),
            ]),
            credential_adapter,
            transport_adapter,
            sensitive_inputs,
        })
    }
}

struct ProbeFacts(u8);

impl ProbeFacts {
    const ENCODED_STREAMING: u8 = 1;
    const EXCHANGE_MATCHED: u8 = 1 << 1;
    const CONFIGURED_SELECTED: u8 = 1 << 2;
    const REQUEST_BOUND_SELECTED: u8 = 1 << 3;

    fn new(observations: &[(u8, bool)]) -> Self {
        Self(observations.iter().fold(
            0,
            |bits, (fact, observed)| {
                if *observed { bits | fact } else { bits }
            },
        ))
    }

    const fn contains(&self, fact: u8) -> bool {
        self.0 & fact != 0
    }
}

struct CanaryCredential {
    bytes: Vec<u8>,
    resolutions: Arc<AtomicUsize>,
    adapter: String,
    observations: Arc<Mutex<Vec<String>>>,
}

impl CredentialSource for CanaryCredential {
    fn resolve(&self, _reference: &CredentialReference) -> Result<Credential, ProviderCoreError> {
        self.resolutions.fetch_add(1, Ordering::SeqCst);
        self.observations
            .lock()
            .map_err(|_| ProviderCoreError::configuration("openai_test", "credential log failed"))?
            .push(self.adapter.clone());
        Credential::new(self.bytes.clone())
    }
}

struct TaggedTransport {
    inner: ReqwestTransport,
    adapter: String,
    observations: Arc<Mutex<Vec<String>>>,
}

impl HttpTransport for TaggedTransport {
    fn send<'a>(
        &'a self,
        request: HttpRequest,
        cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<peritus_provider_core::HttpResponse, ProviderCoreError>> {
        let Ok(mut observations) = self.observations.lock() else {
            return Box::pin(async {
                Err(ProviderCoreError::transport("openai_test", "transport log failed"))
            });
        };
        observations.push(self.adapter.clone());
        drop(observations);
        self.inner.send(request, cancellation)
    }
}

fn script(scenario: ProviderScenario) -> (u16, Vec<u8>, FakeHttpFault) {
    match scenario {
        ProviderScenario::OrderedDeduplication => {
            (200, fixture_bytes("ordered-duplicate.sse"), FakeHttpFault::Complete)
        }
        ProviderScenario::FragmentedToolCall => {
            (200, fixture_bytes("tool-reasoning.sse"), FakeHttpFault::Complete)
        }
        ProviderScenario::MalformedPayload => {
            (200, fixture_bytes("corrupt.sse"), FakeHttpFault::Complete)
        }
        ProviderScenario::IncompleteStream => {
            (200, fixture_bytes("incomplete.sse"), FakeHttpFault::Complete)
        }
        ProviderScenario::Interruption => {
            (200, fixture_bytes("success.sse"), FakeHttpFault::CloseAfterChunks(1))
        }
        ProviderScenario::AuthenticationFailure => {
            (401, fixture_bytes("auth-error.json"), FakeHttpFault::Complete)
        }
        _ => (200, fixture_bytes("success.sse"), FakeHttpFault::Complete),
    }
}
