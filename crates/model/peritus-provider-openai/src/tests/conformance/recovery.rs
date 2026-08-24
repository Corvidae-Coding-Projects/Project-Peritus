//! Direct retry and ambiguity observations through the production request loop.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use peritus_conformance::{
    ProviderAttemptObservation, ProviderAttemptOutcome, ProviderConformanceError,
    ProviderConformanceFixture, ProviderRetryObservation, ProviderTerminal,
};
use peritus_model_protocol::{FailureCategory, ModelEvent};
use peritus_provider_core::{
    BoxFuture, CancellationToken, Endpoint, HttpRequest, HttpTransport, ModelProvider,
    ProviderCoreError, RetryPolicy,
};
use peritus_test_support::{
    ExpectedHttpRequest, FakeHttpExchangeScript, FakeHttpFault, FakeHttpHeader, FakeHttpLimits,
    FakeHttpSequenceServer, HeaderMatchMode, ScriptedHttpResponse,
};

use super::super::support::{
    StaticCredential, block_on, credential_reference, fixture, minimal_request, profile_minimal,
};
use crate::{OpenAiConfig, OpenAiProvider};

type ResponseWorker = std::thread::JoinHandle<Result<(ModelEvent, u64), ProviderConformanceError>>;

#[derive(Clone, Copy)]
pub(super) enum Scenario {
    RateLimit,
    Transient,
    Ambiguous,
}

pub(super) fn observe(
    scenario: Scenario,
    fixture_data: &ProviderConformanceFixture,
) -> Result<ProviderRetryObservation, ProviderConformanceError> {
    match scenario {
        Scenario::RateLimit | Scenario::Transient => observe_http_sequence(scenario, fixture_data),
        Scenario::Ambiguous => observe_ambiguous(),
    }
}

fn observe_http_sequence(
    scenario: Scenario,
    fixture_data: &ProviderConformanceFixture,
) -> Result<ProviderRetryObservation, ProviderConformanceError> {
    let profile = profile_minimal();
    let request = minimal_request(&profile);
    let body =
        crate::request::encode(&request).map_err(|_| ProviderConformanceError::Infrastructure)?;
    let limits = FakeHttpLimits::default();
    let server = FakeHttpSequenceServer::start(sequence(scenario, &body, limits)?, limits)
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let endpoint =
        Endpoint::new(server.base_url()).map_err(|_| ProviderConformanceError::Infrastructure)?;
    let credentials = Arc::new(StaticCredential::new());
    let provider = OpenAiProvider::new(sequence_config(endpoint)?, profile, credentials.clone())
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let started = Instant::now();
    let worker = response_worker(provider, request)?;
    let (terminal, events) =
        worker.join().map_err(|_| ProviderConformanceError::Infrastructure)??;
    let elapsed = u64::try_from(started.elapsed().as_millis())
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let exchanges = server.finish().map_err(|_| ProviderConformanceError::Infrastructure)?;
    if !matches!(terminal, ModelEvent::ResponseCompleted)
        || credentials.resolutions() != 2
        || exchanges.len() != 2
        || exchanges.iter().any(|exchange| {
            !exchange.request().matched() || exchange.request().body_bytes() != body.len()
        })
    {
        return Err(ProviderConformanceError::Infrastructure);
    }
    let delay = match scenario {
        Scenario::RateLimit if elapsed >= fixture_data.retry_after_millis() => {
            fixture_data.retry_after_millis()
        }
        Scenario::Transient if elapsed > 0 && elapsed <= fixture_data.max_retry_delay_millis() => {
            elapsed
        }
        _ => return Err(ProviderConformanceError::Infrastructure),
    };
    let first_outcome = match scenario {
        Scenario::RateLimit => ProviderAttemptOutcome::RateLimited,
        Scenario::Transient => ProviderAttemptOutcome::TransientFailure,
        Scenario::Ambiguous => return Err(ProviderConformanceError::Infrastructure),
    };
    let request_sent = |index: usize| exchanges[index].request().body_bytes() > 0;
    Ok(ProviderRetryObservation::new(
        vec![
            ProviderAttemptObservation::new(1, first_outcome, request_sent(0), 0, 0),
            ProviderAttemptObservation::new(
                2,
                ProviderAttemptOutcome::Completed,
                request_sent(1),
                events,
                delay,
            ),
        ],
        ProviderTerminal::Completed,
        false,
    ))
}

fn sequence(
    scenario: Scenario,
    body: &[u8],
    limits: FakeHttpLimits,
) -> Result<Vec<FakeHttpExchangeScript>, ProviderConformanceError> {
    let first = match scenario {
        Scenario::RateLimit => scripted_response(
            429,
            vec![header("retry-after-ms", "250")?],
            fixture("rate-error.json"),
            limits,
        )?,
        Scenario::Transient => {
            scripted_response(503, Vec::new(), fixture("transient-error.json"), limits)?
        }
        Scenario::Ambiguous => return Err(ProviderConformanceError::Infrastructure),
    };
    let success = scripted_response(
        200,
        vec![header("content-type", "text/event-stream")?],
        fixture("success.sse"),
        limits,
    )?;
    Ok(vec![
        FakeHttpExchangeScript::new(expectation(body.to_vec(), limits)?, first),
        FakeHttpExchangeScript::new(expectation(body.to_vec(), limits)?, success),
    ])
}

fn expectation(
    body: Vec<u8>,
    limits: FakeHttpLimits,
) -> Result<ExpectedHttpRequest, ProviderConformanceError> {
    ExpectedHttpRequest::new("POST", "/v1/responses", Vec::new(), body, limits)
        .map(|expected| expected.header_match_mode(HeaderMatchMode::AllowAdditional))
        .map_err(|_| ProviderConformanceError::Infrastructure)
}

fn scripted_response(
    status: u16,
    headers: Vec<FakeHttpHeader>,
    body: Vec<u8>,
    limits: FakeHttpLimits,
) -> Result<ScriptedHttpResponse, ProviderConformanceError> {
    ScriptedHttpResponse::new(status, headers, vec![body], FakeHttpFault::Complete, None, limits)
        .map_err(|_| ProviderConformanceError::Infrastructure)
}

fn header(name: &str, value: &str) -> Result<FakeHttpHeader, ProviderConformanceError> {
    FakeHttpHeader::new(name, value).map_err(|_| ProviderConformanceError::Infrastructure)
}

fn sequence_config(endpoint: Endpoint) -> Result<OpenAiConfig, ProviderConformanceError> {
    let policy = RetryPolicy::new(
        2,
        [
            Duration::from_millis(1),
            Duration::from_millis(300),
            Duration::from_millis(300),
            Duration::from_secs(1),
        ],
        1024 * 1024,
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)?;
    OpenAiConfig::for_test(endpoint, credential_reference())
        .map(|config| config.with_retry_policy(policy))
        .map_err(|_| ProviderConformanceError::Infrastructure)
}

fn response_worker(
    provider: OpenAiProvider,
    request: peritus_model_protocol::ModelRequest,
) -> Result<ResponseWorker, ProviderConformanceError> {
    std::thread::Builder::new()
        .name("openai-retry-conformance".to_owned())
        .spawn(move || {
            block_on(async {
                let mut stream = provider
                    .start(request, CancellationToken::new())
                    .await
                    .map_err(|_| ProviderConformanceError::Infrastructure)?;
                let mut events = 0_u64;
                loop {
                    let event = stream
                        .pull()
                        .await
                        .map_err(|_| ProviderConformanceError::Infrastructure)?
                        .ok_or(ProviderConformanceError::Infrastructure)?;
                    events = events.saturating_add(1);
                    if matches!(
                        event.event(),
                        ModelEvent::ResponseCompleted | ModelEvent::ResponseFailed(_)
                    ) {
                        return Ok((event.event().clone(), events));
                    }
                }
            })
        })
        .map_err(|_| ProviderConformanceError::Infrastructure)
}

fn observe_ambiguous() -> Result<ProviderRetryObservation, ProviderConformanceError> {
    let profile = profile_minimal();
    let request = minimal_request(&profile);
    let transport = Arc::new(AmbiguousTransport::default());
    let credentials = Arc::new(StaticCredential::new());
    let config = OpenAiConfig::for_test(
        Endpoint::new("http://127.0.0.1:9".to_owned())
            .map_err(|_| ProviderConformanceError::Infrastructure)?,
        credential_reference(),
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let provider =
        OpenAiProvider::with_transport(config, profile, credentials.clone(), transport.clone())
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let (terminal, events) = response_worker(provider, request)?
        .join()
        .map_err(|_| ProviderConformanceError::Infrastructure)??;
    let ambiguous = matches!(
        terminal,
        ModelEvent::ResponseFailed(ref failure)
            if failure.category() == FailureCategory::AmbiguousAcceptance
    );
    if !ambiguous
        || transport.requests.load(Ordering::SeqCst) != 1
        || credentials.resolutions() != 1
    {
        return Err(ProviderConformanceError::Infrastructure);
    }
    Ok(ProviderRetryObservation::new(
        vec![ProviderAttemptObservation::new(
            1,
            ProviderAttemptOutcome::Ambiguous,
            transport.body_bytes.load(Ordering::SeqCst) > 0,
            events,
            0,
        )],
        ProviderTerminal::Failed,
        true,
    ))
}

#[derive(Default)]
struct AmbiguousTransport {
    requests: AtomicU64,
    body_bytes: AtomicUsize,
}

impl HttpTransport for AmbiguousTransport {
    fn send<'a>(
        &'a self,
        request: HttpRequest,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<peritus_provider_core::HttpResponse, ProviderCoreError>> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        self.body_bytes.store(request.body().len(), Ordering::SeqCst);
        Box::pin(async {
            Err(ProviderCoreError::transport("openai_conformance", "submission outcome is unknown"))
        })
    }
}
