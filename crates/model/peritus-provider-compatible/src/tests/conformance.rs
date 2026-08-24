//! Development-only A2 qualification bridge over production clients and fresh fake servers.

mod observations;
mod recovery;
mod redaction;
mod runner;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use peritus_conformance::{
    ProviderCancellationObservation, ProviderConformanceError, ProviderConformanceFixture,
    ProviderConformanceObservation, ProviderConformanceSubject, ProviderFailureKind,
    ProviderScenario, ProviderTerminal,
};
use peritus_model_protocol::{Capability, EventEnvelope};
use peritus_provider_core::{
    BoxFuture, CancellationToken, Credential, CredentialReference, CredentialSource, Endpoint,
    HttpRequest, HttpTransport, ModelProvider, ProviderCoreError, ReqwestTransport,
};
use peritus_test_support::{
    ExpectedHttpRequest, FakeHttpFault, FakeHttpHeader, FakeHttpLimits, FakeHttpServer,
    HeaderMatchMode, ScriptedHttpResponse,
};

use super::support::{
    block_on, credential_reference, fixture as fixture_bytes, minimal_request, redaction_request,
    request_with_id, responses_profile, tool_request,
};
use crate::{CompatibleAuth, CompatibleClient, CompatibleConfig, CompatibleProfile};

pub(super) struct Subject;

impl ProviderConformanceSubject for Subject {
    fn exercise(
        &mut self,
        fixture: &ProviderConformanceFixture,
    ) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
        let probe = Probe::run(fixture)?;
        Ok(match fixture.scenario() {
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
        })
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
        reason = "one production probe binds a fake exchange to every captured observation"
    )]
    fn run(fixture: &ProviderConformanceFixture) -> Result<Self, ProviderConformanceError> {
        let scenario = fixture.scenario();
        let (status, bytes, fault) = script(scenario);
        let capabilities = match scenario {
            ProviderScenario::CapabilityHonesty => vec![Capability::Streaming],
            ProviderScenario::FragmentedToolCall | ProviderScenario::Redaction => {
                vec![Capability::Streaming, Capability::ToolCalls, Capability::UsageDetail]
            }
            _ => vec![Capability::Streaming, Capability::UsageDetail],
        };
        let provider_profile = responses_profile(&capabilities);
        let profile = CompatibleProfile::responses(provider_profile.clone())
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let request = match scenario {
            ProviderScenario::Redaction => redaction_request(&provider_profile, fixture.canary()),
            ProviderScenario::AdapterIsolation => {
                request_with_id(&provider_profile, fixture.selected_adapter())
            }
            ProviderScenario::FragmentedToolCall => tool_request(&provider_profile),
            _ => minimal_request(&provider_profile),
        };
        let request_bound_selected =
            request.request_id().expose_for_wire() == fixture.selected_adapter();
        let request_sensitive_inputs = if scenario == ProviderScenario::Redaction {
            redaction::request_canary_count(&request, fixture.canary())
        } else {
            0
        };
        let request_surface = format!("{request:?}");
        let body = crate::request::encode(&profile, &request)
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let encoded_streaming = serde_json::from_slice::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| value.get("stream").and_then(serde_json::Value::as_bool))
            == Some(true);
        let limits = FakeHttpLimits::default();
        let expected =
            ExpectedHttpRequest::new("POST", "/compatible/responses", Vec::new(), body, limits)
                .map_err(|_| ProviderConformanceError::Infrastructure)?
                .header_match_mode(HeaderMatchMode::AllowAdditional);
        let chunks = if scenario == ProviderScenario::Interruption {
            let midpoint = bytes.len() / 2;
            vec![bytes[..midpoint].to_vec(), bytes[midpoint..].to_vec()]
        } else {
            bytes.chunks(23).map(<[u8]>::to_vec).collect()
        };
        let mut headers = vec![
            FakeHttpHeader::new("content-type", "text/event-stream")
                .map_err(|_| ProviderConformanceError::Infrastructure)?,
        ];
        let response_canary = scenario == ProviderScenario::Redaction;
        if response_canary {
            headers.push(
                FakeHttpHeader::new("set-cookie", fixture.canary())
                    .map_err(|_| ProviderConformanceError::Infrastructure)?,
            );
        }
        let response = ScriptedHttpResponse::new(status, headers, chunks, fault, None, limits)
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let server = FakeHttpServer::start(expected, response, limits)
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let endpoint = Endpoint::new(format!(
            "{}/compatible/responses",
            server.base_url().trim_end_matches('/')
        ))
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let auth = CompatibleAuth::bearer(credential_reference())
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let config = CompatibleConfig::new(endpoint, auth)
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let configured_selected = config.endpoint().as_str().ends_with("/compatible/responses");
        let config_surface = format!("{config:?}");
        let secret = if scenario == ProviderScenario::Redaction {
            fixture.canary().as_bytes()
        } else {
            b"compatible-conformance-key"
        };
        let credential_canary = secret == fixture.canary().as_bytes();
        let resolutions = Arc::new(AtomicUsize::new(0));
        let label = if scenario == ProviderScenario::AdapterIsolation {
            fixture.selected_adapter()
        } else {
            "compatible-responses"
        };
        let credential_log = Arc::new(Mutex::new(Vec::new()));
        let transport_log = Arc::new(Mutex::new(Vec::new()));
        let transport = ReqwestTransport::new(config.http_limits())
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let client = CompatibleClient::with_transport(
            config,
            profile,
            Arc::new(CanaryCredential {
                bytes: secret.to_vec(),
                resolutions: Arc::clone(&resolutions),
                adapter: label.to_owned(),
                observations: Arc::clone(&credential_log),
            }),
            Arc::new(TaggedTransport {
                inner: transport,
                adapter: label.to_owned(),
                observations: Arc::clone(&transport_log),
            }),
        );
        let worker = std::thread::Builder::new()
            .name("compatible-conformance".to_owned())
            .spawn(move || {
                block_on(async {
                    let mut stream = client
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
                    Ok::<_, ProviderConformanceError>((events, format!("{client:?}")))
                })
            })
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
        let (events, diagnostic) =
            worker.join().map_err(|_| ProviderConformanceError::Infrastructure)??;
        let exchange = server.finish().map_err(|_| ProviderConformanceError::Infrastructure)?;
        if !exchange.request().matched() {
            return Err(ProviderConformanceError::Infrastructure);
        }
        let mut surfaces =
            vec![request_surface, config_surface, diagnostic, format!("{exchange:?}")];
        surfaces.extend(events.iter().map(|event| format!("{event:?}")));
        let credential_adapter = credential_log
            .lock()
            .map_err(|_| ProviderConformanceError::Infrastructure)?
            .first()
            .cloned();
        let transport_adapter = transport_log
            .lock()
            .map_err(|_| ProviderConformanceError::Infrastructure)?
            .first()
            .cloned();
        let transport_requests = u64::try_from(
            transport_log.lock().map_err(|_| ProviderConformanceError::Infrastructure)?.len(),
        )
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
        Ok(Self {
            events,
            surfaces,
            transport_requests,
            credential_resolutions: resolutions.load(Ordering::SeqCst),
            facts: ProbeFacts::new(&[
                (ProbeFacts::ENCODED_STREAMING, encoded_streaming),
                (ProbeFacts::EXCHANGE_MATCHED, true),
                (ProbeFacts::CONFIGURED_SELECTED, configured_selected),
                (ProbeFacts::REQUEST_BOUND_SELECTED, request_bound_selected),
            ]),
            credential_adapter,
            transport_adapter,
            sensitive_inputs: request_sensitive_inputs
                + usize::from(credential_canary)
                + usize::from(response_canary),
        })
    }
}

struct ProbeFacts(u8);

impl ProbeFacts {
    const ENCODED_STREAMING: u8 = 1;
    const EXCHANGE_MATCHED: u8 = 1 << 1;
    const CONFIGURED_SELECTED: u8 = 1 << 2;
    const REQUEST_BOUND_SELECTED: u8 = 1 << 3;

    fn new(values: &[(u8, bool)]) -> Self {
        Self(values.iter().fold(
            0,
            |bits, (flag, observed)| {
                if *observed { bits | flag } else { bits }
            },
        ))
    }

    const fn contains(&self, flag: u8) -> bool {
        self.0 & flag != 0
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
            .map_err(|_| ProviderCoreError::configuration("compatible_test", "log failed"))?
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
                Err(ProviderCoreError::transport("compatible_test", "log failed"))
            });
        };
        observations.push(self.adapter.clone());
        drop(observations);
        self.inner.send(request, cancellation)
    }
}

fn script(scenario: ProviderScenario) -> (u16, Vec<u8>, FakeHttpFault) {
    match scenario {
        ProviderScenario::CapabilityHonesty => {
            (200, fixture_bytes("responses-no-usage.sse"), FakeHttpFault::Complete)
        }
        ProviderScenario::OrderedDeduplication => {
            (200, fixture_bytes("responses-ordered-duplicate.sse"), FakeHttpFault::Complete)
        }
        ProviderScenario::FragmentedToolCall => {
            (200, fixture_bytes("responses-tool.sse"), FakeHttpFault::Complete)
        }
        ProviderScenario::MalformedPayload => {
            (200, fixture_bytes("responses-corrupt.sse"), FakeHttpFault::Complete)
        }
        ProviderScenario::IncompleteStream => {
            (200, fixture_bytes("responses-incomplete.sse"), FakeHttpFault::Complete)
        }
        ProviderScenario::Interruption => {
            (200, fixture_bytes("responses-success.sse"), FakeHttpFault::CloseAfterChunks(1))
        }
        ProviderScenario::AuthenticationFailure => {
            (401, fixture_bytes("auth-error.json"), FakeHttpFault::Complete)
        }
        _ => (200, fixture_bytes("responses-success.sse"), FakeHttpFault::Complete),
    }
}
