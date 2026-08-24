//! Retry and ambiguity observations derived from production exchanges.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use peritus_conformance::{
    ProviderAttemptObservation, ProviderAttemptOutcome, ProviderConformanceError,
    ProviderConformanceFixture, ProviderRetryObservation, ProviderTerminal,
};
use peritus_model_protocol::{Capability, FailureCategory, ModelEvent};
use peritus_provider_core::{
    BoxFuture, CancellationToken, Endpoint, HttpRequest, HttpTransport, ModelProvider,
    ProviderCoreError, RetryPolicy,
};
use peritus_test_support::{
    ExpectedHttpRequest, FakeHttpExchangeScript, FakeHttpFault, FakeHttpHeader, FakeHttpLimits,
    FakeHttpSequenceServer, HeaderMatchMode, ScriptedHttpResponse,
};

use super::super::support::{
    StaticCredential, block_on, credential_reference, fixture, minimal_request, responses_profile,
};
use crate::{
    CompatibleAuth, CompatibleClient, CompatibleConfig, CompatibleProfile, CompatibleRetryStatuses,
};

type Worker = std::thread::JoinHandle<Result<(ModelEvent, u64), ProviderConformanceError>>;

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
        Scenario::RateLimit | Scenario::Transient => http_sequence(scenario, fixture_data),
        Scenario::Ambiguous => ambiguous(),
    }
}

fn http_sequence(
    scenario: Scenario,
    fixture_data: &ProviderConformanceFixture,
) -> Result<ProviderRetryObservation, ProviderConformanceError> {
    let provider_profile = responses_profile(&[Capability::Streaming, Capability::UsageDetail]);
    let profile = CompatibleProfile::responses(provider_profile.clone())
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let request = minimal_request(&provider_profile);
    let body = crate::request::encode(&profile, &request)
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let limits = FakeHttpLimits::default();
    let server = FakeHttpSequenceServer::start(sequence(scenario, &body, limits)?, limits)
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let endpoint =
        Endpoint::new(format!("{}/compatible/retry", server.base_url().trim_end_matches('/')))
            .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let credentials = Arc::new(StaticCredential::new());
    let client = CompatibleClient::new(config(endpoint)?, profile, credentials.clone())
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let started = Instant::now();
    let (terminal, events) =
        worker(client, request)?.join().map_err(|_| ProviderConformanceError::Infrastructure)??;
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
    let outcome = match scenario {
        Scenario::RateLimit => ProviderAttemptOutcome::RateLimited,
        Scenario::Transient => ProviderAttemptOutcome::TransientFailure,
        Scenario::Ambiguous => return Err(ProviderConformanceError::Infrastructure),
    };
    Ok(ProviderRetryObservation::new(
        vec![
            ProviderAttemptObservation::new(
                1,
                outcome,
                exchanges[0].request().body_bytes() > 0,
                0,
                0,
            ),
            ProviderAttemptObservation::new(
                2,
                ProviderAttemptOutcome::Completed,
                exchanges[1].request().body_bytes() > 0,
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
        Scenario::RateLimit => {
            response(429, vec![header("retry-after", "1")?], fixture("rate-error.json"), limits)?
        }
        Scenario::Transient => response(503, Vec::new(), fixture("transient-error.json"), limits)?,
        Scenario::Ambiguous => return Err(ProviderConformanceError::Infrastructure),
    };
    let success = response(
        200,
        vec![header("content-type", "text/event-stream")?],
        fixture("responses-success.sse"),
        limits,
    )?;
    Ok(vec![
        FakeHttpExchangeScript::new(expectation(body.to_vec(), limits)?, first),
        FakeHttpExchangeScript::new(expectation(body.to_vec(), limits)?, success),
    ])
}

fn config(endpoint: Endpoint) -> Result<CompatibleConfig, ProviderConformanceError> {
    let retry = RetryPolicy::new(
        2,
        [
            Duration::from_millis(1),
            Duration::from_secs(2),
            Duration::from_secs(2),
            Duration::from_secs(2),
        ],
        1024 * 1024,
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let auth = CompatibleAuth::bearer(credential_reference())
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    CompatibleConfig::new(endpoint, auth)
        .map(|value| {
            value.with_retry_policy(retry).with_retry_statuses(
                CompatibleRetryStatuses::none().with_rate_limited().with_server_errors(),
            )
        })
        .map_err(|_| ProviderConformanceError::Infrastructure)
}

fn worker(
    client: CompatibleClient,
    request: peritus_model_protocol::ModelRequest,
) -> Result<Worker, ProviderConformanceError> {
    std::thread::Builder::new()
        .name("compatible-retry-conformance".to_owned())
        .spawn(move || {
            block_on(async {
                let mut stream = client
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

fn ambiguous() -> Result<ProviderRetryObservation, ProviderConformanceError> {
    let provider_profile = responses_profile(&[Capability::Streaming]);
    let profile = CompatibleProfile::responses(provider_profile.clone())
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let request = minimal_request(&provider_profile);
    let transport = Arc::new(AmbiguousTransport::default());
    let credentials = Arc::new(StaticCredential::new());
    let endpoint = Endpoint::new("http://127.0.0.1:9/compatible".to_owned())
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let auth = CompatibleAuth::bearer(credential_reference())
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let config = CompatibleConfig::new(endpoint, auth)
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let client =
        CompatibleClient::with_transport(config, profile, credentials.clone(), transport.clone());
    let (terminal, events) =
        worker(client, request)?.join().map_err(|_| ProviderConformanceError::Infrastructure)??;
    if !matches!(
        terminal,
        ModelEvent::ResponseFailed(ref failure)
            if failure.category() == FailureCategory::AmbiguousAcceptance
    ) || transport.requests.load(Ordering::SeqCst) != 1
        || credentials.resolutions() != 1
    {
        return Err(ProviderConformanceError::Infrastructure);
    }
    Ok(ProviderRetryObservation::new(
        vec![ProviderAttemptObservation::new(
            1,
            ProviderAttemptOutcome::Ambiguous,
            transport.bytes.load(Ordering::SeqCst) > 0,
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
    bytes: AtomicUsize,
}

impl HttpTransport for AmbiguousTransport {
    fn send<'a>(
        &'a self,
        request: HttpRequest,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<peritus_provider_core::HttpResponse, ProviderCoreError>> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        self.bytes.store(request.body().len(), Ordering::SeqCst);
        Box::pin(async { Err(ProviderCoreError::transport("compatible_test", "ambiguous")) })
    }
}

fn expectation(
    body: Vec<u8>,
    limits: FakeHttpLimits,
) -> Result<ExpectedHttpRequest, ProviderConformanceError> {
    ExpectedHttpRequest::new("POST", "/compatible/retry", Vec::new(), body, limits)
        .map(|value| value.header_match_mode(HeaderMatchMode::AllowAdditional))
        .map_err(|_| ProviderConformanceError::Infrastructure)
}

fn response(
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
