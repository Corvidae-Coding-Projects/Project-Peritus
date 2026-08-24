//! Real-loopback retry and explicit ambiguous-submission observations.

use std::sync::Arc;
use std::time::Instant;

use peritus_conformance::{
    ProviderAttemptObservation, ProviderAttemptOutcome, ProviderConformanceError,
    ProviderConformanceFixture, ProviderConformanceObservation, ProviderRetryObservation,
    ProviderTerminal,
};
use peritus_model_protocol::{FailureCategory, ModelEvent, WireDialect};
use peritus_provider_core::{CancellationToken, ModelProvider, ProviderCoreError};
use peritus_test_support::{
    ExpectedHttpRequest, FakeHttpExchangeScript, FakeHttpFault, FakeHttpHeader, FakeHttpLimits,
    FakeHttpSequenceServer, HeaderMatchMode, ScriptedHttpResponse,
};

use crate::GoogleClient;
use crate::test_support::{
    TestCredentials, TestTransport, TransportState, block_on, config, config_at, fixture, profile,
    request,
};

pub(super) fn observe_rate(
    fixture: &ProviderConformanceFixture,
) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
    observe_http(
        429,
        vec![header("retry-after", "0.250")?],
        "rate_error.json",
        ProviderAttemptOutcome::RateLimited,
        fixture.retry_after_millis(),
    )
}

pub(super) fn observe_transient(
    fixture: &ProviderConformanceFixture,
) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
    let observed = observe_http(
        503,
        Vec::new(),
        "transient_error.json",
        ProviderAttemptOutcome::TransientFailure,
        1,
    )?;
    let ProviderConformanceObservation::Retry(retry) = &observed else {
        return Err(ProviderConformanceError::Infrastructure);
    };
    if retry.attempts()[1].delay_before_millis() > fixture.max_retry_delay_millis() {
        return Err(ProviderConformanceError::Infrastructure);
    }
    Ok(observed)
}

pub(super) fn observe_ambiguous() -> Result<ProviderConformanceObservation, ProviderConformanceError>
{
    let state = TransportState::with_responses(vec![Err(ProviderCoreError::transport(
        "google_conformance",
        "submission outcome is unknown",
    ))]);
    let dialect = WireDialect::GeminiInteractionsV1;
    let client = GoogleClient::with_transport(
        config(dialect, 2),
        Box::new(TestCredentials::default()),
        Box::new(TestTransport(Arc::clone(&state))),
    );
    let terminal = run(client, request(&profile(dialect), true))?;
    match (terminal, state.captures().len()) {
        (ModelEvent::ResponseFailed(failure), 1)
            if failure.category() == FailureCategory::AmbiguousAcceptance =>
        {
            Ok(ProviderConformanceObservation::Retry(ProviderRetryObservation::new(
                vec![ProviderAttemptObservation::new(
                    1,
                    ProviderAttemptOutcome::Ambiguous,
                    true,
                    0,
                    0,
                )],
                ProviderTerminal::Failed,
                true,
            )))
        }
        _ => Err(ProviderConformanceError::Infrastructure),
    }
}

fn observe_http(
    status: u16,
    headers: Vec<FakeHttpHeader>,
    failure_fixture: &str,
    first_outcome: ProviderAttemptOutcome,
    expected_delay: u64,
) -> Result<ProviderConformanceObservation, ProviderConformanceError> {
    let dialect = WireDialect::GeminiInteractionsV1;
    let profile = profile(dialect);
    let request = request(&profile, true);
    let body = crate::request::encode(&request, config(dialect, 2).endpoint())
        .map_err(|_| ProviderConformanceError::Infrastructure)?
        .body;
    let limits = FakeHttpLimits::default();
    let scripts = vec![
        step(&body, status, headers, fixture(failure_fixture), limits)?,
        step(
            &body,
            200,
            vec![header("content-type", "text/event-stream")?],
            fixture("interactions_success.sse"),
            limits,
        )?,
    ];
    let server = FakeHttpSequenceServer::start(scripts, limits)
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let client = GoogleClient::new(
        config_at(&server.base_url(), dialect, 2),
        Box::new(TestCredentials::default()),
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let started = Instant::now();
    let terminal = run(client, request)?;
    let elapsed = u64::try_from(started.elapsed().as_millis())
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let exchanges = server.finish().map_err(|_| ProviderConformanceError::Infrastructure)?;
    if !matches!(terminal, ModelEvent::ResponseCompleted)
        || exchanges.len() != 2
        || exchanges.iter().any(|exchange| !exchange.request().matched())
        || elapsed < expected_delay
    {
        return Err(ProviderConformanceError::Infrastructure);
    }
    Ok(ProviderConformanceObservation::Retry(ProviderRetryObservation::new(
        vec![
            ProviderAttemptObservation::new(1, first_outcome, true, 0, 0),
            ProviderAttemptObservation::new(
                2,
                ProviderAttemptOutcome::Completed,
                true,
                1,
                expected_delay,
            ),
        ],
        ProviderTerminal::Completed,
        false,
    )))
}

fn run(
    client: GoogleClient,
    request: peritus_model_protocol::ModelRequest,
) -> Result<ModelEvent, ProviderConformanceError> {
    let worker = std::thread::Builder::new()
        .name("google-recovery-conformance".to_owned())
        .spawn(move || {
            block_on(async {
                let mut stream = client
                    .start(request, CancellationToken::new())
                    .await
                    .map_err(|_| ProviderConformanceError::Infrastructure)?;
                loop {
                    let event = stream
                        .pull()
                        .await
                        .map_err(|_| ProviderConformanceError::Infrastructure)?
                        .ok_or(ProviderConformanceError::Infrastructure)?;
                    if matches!(
                        event.event(),
                        ModelEvent::ResponseCompleted | ModelEvent::ResponseFailed(_)
                    ) {
                        return Ok(event.event().clone());
                    }
                }
            })
        })
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    worker.join().map_err(|_| ProviderConformanceError::Infrastructure)?
}

fn step(
    body: &[u8],
    status: u16,
    headers: Vec<FakeHttpHeader>,
    response_body: Vec<u8>,
    limits: FakeHttpLimits,
) -> Result<FakeHttpExchangeScript, ProviderConformanceError> {
    let expected = ExpectedHttpRequest::new("POST", "/v1/interactions", Vec::new(), body, limits)
        .map_err(|_| ProviderConformanceError::Infrastructure)?
        .header_match_mode(HeaderMatchMode::AllowAdditional);
    let response = ScriptedHttpResponse::new(
        status,
        headers,
        vec![response_body],
        FakeHttpFault::Complete,
        None,
        limits,
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)?;
    Ok(FakeHttpExchangeScript::new(expected, response))
}

fn header(name: &str, value: &str) -> Result<FakeHttpHeader, ProviderConformanceError> {
    FakeHttpHeader::new(name, value).map_err(|_| ProviderConformanceError::Infrastructure)
}
