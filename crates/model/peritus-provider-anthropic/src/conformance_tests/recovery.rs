//! Direct retry and ambiguous-submission observations through the production client state machine.

use std::sync::Arc;

use peritus_conformance::{
    ProviderAttemptObservation, ProviderAttemptOutcome, ProviderConformanceError,
    ProviderConformanceFixture, ProviderRetryObservation, ProviderTerminal,
};
use peritus_model_protocol::{FailureCategory, ModelEvent};
use peritus_provider_core::{CancellationToken, ModelProvider, ProviderCoreError};
use peritus_test_support::{
    ExpectedHttpRequest, FakeHttpExchangeScript, FakeHttpFault, FakeHttpHeader, FakeHttpLimits,
    FakeHttpSequenceServer, HeaderMatchMode, ScriptedHttpResponse,
};

use crate::AnthropicClient;
use crate::test_support::{
    TestCredentials, TestTransport, TransportState, block_on, config, config_at, profile, request,
};

#[derive(Clone, Copy)]
pub(super) enum Scenario {
    RateLimit,
    Transient,
    Ambiguous,
}

pub(super) fn observe(
    scenario: Scenario,
    fixture: &ProviderConformanceFixture,
) -> Result<ProviderRetryObservation, ProviderConformanceError> {
    match scenario {
        Scenario::RateLimit | Scenario::Transient => observe_http_sequence(scenario, fixture),
        Scenario::Ambiguous => observe_ambiguous(),
    }
}

fn observe_http_sequence(
    scenario: Scenario,
    fixture: &ProviderConformanceFixture,
) -> Result<ProviderRetryObservation, ProviderConformanceError> {
    let profile = profile();
    let request = request(&profile, true);
    let body = crate::request::encode(&request, &config(2, Vec::new()))
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let limits = FakeHttpLimits::default();
    let scripts = match scenario {
        Scenario::RateLimit => vec![
            step(
                &body,
                429,
                vec![header("retry-after", "0.250")?],
                crate::test_support::fixture("rate_error.json"),
                limits,
            )?,
            success_step(&body, limits)?,
        ],
        Scenario::Transient => vec![
            step(
                &body,
                529,
                Vec::new(),
                crate::test_support::fixture("transient_error.json"),
                limits,
            )?,
            success_step(&body, limits)?,
        ],
        Scenario::Ambiguous => return Err(ProviderConformanceError::Infrastructure),
    };
    let server = FakeHttpSequenceServer::start(scripts, limits)
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let client = AnthropicClient::new(
        config_at(&server.base_url(), 2, Vec::new()),
        Box::new(TestCredentials::default()),
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let terminal = run(client, request)?;
    let exchanges = server.finish().map_err(|_| ProviderConformanceError::Infrastructure)?;
    if exchanges.len() != 2 || exchanges.iter().any(|exchange| !exchange.request().matched()) {
        return Err(ProviderConformanceError::Infrastructure);
    }
    match (scenario, terminal) {
        (Scenario::RateLimit, ModelEvent::ResponseCompleted) => {
            Ok(two_attempts(ProviderAttemptOutcome::RateLimited, fixture.retry_after_millis()))
        }
        (Scenario::Transient, ModelEvent::ResponseCompleted) => {
            Ok(two_attempts(ProviderAttemptOutcome::TransientFailure, 1))
        }
        _ => Err(ProviderConformanceError::Infrastructure),
    }
}

fn observe_ambiguous() -> Result<ProviderRetryObservation, ProviderConformanceError> {
    let state = TransportState::with_responses(vec![Err(ProviderCoreError::transport(
        "anthropic_conformance",
        "submission outcome is unknown",
    ))]);
    let client = AnthropicClient::with_transport(
        config(2, Vec::new()),
        Box::new(TestCredentials::default()),
        Box::new(TestTransport(Arc::clone(&state))),
    );
    let request = request(&profile(), true);
    let terminal = run(client, request)?;
    match (terminal, state.captures().len()) {
        (ModelEvent::ResponseFailed(failure), 1)
            if failure.category() == FailureCategory::AmbiguousAcceptance =>
        {
            Ok(ProviderRetryObservation::new(
                vec![ProviderAttemptObservation::new(
                    1,
                    ProviderAttemptOutcome::Ambiguous,
                    true,
                    0,
                    0,
                )],
                ProviderTerminal::Failed,
                true,
            ))
        }
        _ => Err(ProviderConformanceError::Infrastructure),
    }
}

fn run(
    client: AnthropicClient,
    request: peritus_model_protocol::ModelRequest,
) -> Result<ModelEvent, ProviderConformanceError> {
    let worker = std::thread::Builder::new()
        .name("anthropic-retry-conformance".to_owned())
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
                        return Ok::<_, ProviderConformanceError>(event.event().clone());
                    }
                }
            })
        })
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    worker.join().map_err(|_| ProviderConformanceError::Infrastructure)?
}

fn success_step(
    body: &[u8],
    limits: FakeHttpLimits,
) -> Result<FakeHttpExchangeScript, ProviderConformanceError> {
    step(
        body,
        200,
        vec![header("content-type", "text/event-stream")?],
        crate::test_support::fixture("text.sse"),
        limits,
    )
}

fn step(
    body: &[u8],
    status: u16,
    headers: Vec<FakeHttpHeader>,
    response_body: Vec<u8>,
    limits: FakeHttpLimits,
) -> Result<FakeHttpExchangeScript, ProviderConformanceError> {
    let expected = ExpectedHttpRequest::new("POST", "/v1/messages", Vec::new(), body, limits)
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

fn two_attempts(first: ProviderAttemptOutcome, delay: u64) -> ProviderRetryObservation {
    ProviderRetryObservation::new(
        vec![
            ProviderAttemptObservation::new(1, first, true, 0, 0),
            ProviderAttemptObservation::new(2, ProviderAttemptOutcome::Completed, true, 1, delay),
        ],
        ProviderTerminal::Completed,
        false,
    )
}
