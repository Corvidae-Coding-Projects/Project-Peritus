//! Authenticated submission, HTTP classification, and retry-boundary tests.

use std::sync::atomic::Ordering;

use peritus_model_protocol::{FailureCategory, ModelEvent};
use peritus_provider_core::{
    CancellationToken, HttpMethod, ModelProvider, ProviderCoreError, ProviderCoreErrorKind,
};

use super::AnthropicClient;
use crate::AnthropicBeta;
use crate::test_support::{
    TestCredentials, TestTransport, TransportState, block_on, config, profile, request, response,
};

fn client(
    responses: Vec<Result<peritus_provider_core::HttpResponse, ProviderCoreError>>,
    attempts: u32,
    betas: Vec<AnthropicBeta>,
) -> (AnthropicClient, std::sync::Arc<TransportState>, std::sync::Arc<std::sync::atomic::AtomicUsize>)
{
    let state = TransportState::with_responses(responses);
    let credentials = TestCredentials::default();
    let resolutions = credentials.counter();
    let client = AnthropicClient::with_transport(
        config(attempts, betas),
        Box::new(credentials),
        Box::new(TestTransport(std::sync::Arc::clone(&state))),
    );
    (client, state, resolutions)
}

async fn terminal_event(
    client: &AnthropicClient,
    request: peritus_model_protocol::ModelRequest,
) -> ModelEvent {
    let mut stream = client.start(request, CancellationToken::new()).await.expect("start");
    loop {
        let event = stream.pull().await.expect("pull").expect("event");
        if matches!(
            event.event(),
            ModelEvent::ResponseCompleted
                | ModelEvent::ResponseFailed(_)
                | ModelEvent::ResponseCancelled
        ) {
            return event.event().clone();
        }
    }
}

#[test]
fn sends_current_messages_contract_with_immediate_redacted_credentials() {
    let success = response(
        200,
        &[("content-type", "text/event-stream; charset=utf-8")],
        vec![crate::test_support::fixture("text.sse")],
    );
    let (client, state, resolutions) =
        client(vec![Ok(success)], 1, vec![AnthropicBeta::PromptCaching20240731]);
    let request = request(&profile(), true);
    let terminal = block_on(terminal_event(&client, request));
    assert!(matches!(terminal, ModelEvent::ResponseCompleted));
    assert_eq!(resolutions.load(Ordering::SeqCst), 1);
    let recorded = state.captures();
    let captured = &recorded[0];
    assert_eq!(captured.method, HttpMethod::Post);
    assert_eq!(captured.endpoint, "https://api.anthropic.com/v1/messages");
    assert!(
        captured
            .body
            .windows(b"claude-sonnet-4-5".len())
            .any(|bytes| bytes == b"claude-sonnet-4-5")
    );
    let api_key = captured.headers.iter().find(|header| header.0 == "x-api-key").expect("API key");
    assert!(api_key.1);
    assert!(api_key.2.is_none());
    assert!(captured.headers.iter().any(|header| {
        header.0 == "anthropic-version" && header.2.as_deref() == Some(b"2023-06-01")
    }));
    assert!(captured.headers.iter().any(|header| {
        header.0 == "anthropic-beta" && header.2.as_deref() == Some(b"prompt-caching-2024-07-31")
    }));
    let diagnostic = format!("{client:?} {captured:?}");
    assert!(!diagnostic.contains("sk-ant-secret-marker"));
}

#[test]
fn missing_stream_negotiation_fails_before_credentials_or_transport() {
    let (client, state, resolutions) = client(Vec::new(), 1, Vec::new());
    let error = block_on(client.start(request(&profile(), false), CancellationToken::new()))
        .expect_err("streaming not negotiated");
    assert_eq!(error.kind(), ProviderCoreErrorKind::InvalidRequest);
    assert_eq!(resolutions.load(Ordering::SeqCst), 0);
    assert!(state.captures().is_empty());
}

#[test]
fn connect_is_retried_but_ambiguous_transport_is_not() {
    let success = response(
        200,
        &[("content-type", "text/event-stream")],
        vec![crate::test_support::fixture("text.sse")],
    );
    let (retrying, state, resolutions) = client(
        vec![
            Err(ProviderCoreError::connect("test", "pre-submission connection failed")),
            Ok(success),
        ],
        2,
        Vec::new(),
    );
    let event = block_on(terminal_event(&retrying, request(&profile(), true)));
    assert!(matches!(event, ModelEvent::ResponseCompleted));
    assert_eq!(state.captures().len(), 2);
    assert_eq!(resolutions.load(Ordering::SeqCst), 2);

    let (ambiguous, state, _) = client(
        vec![Err(ProviderCoreError::transport("test", "submission outcome is unknown"))],
        2,
        Vec::new(),
    );
    let event = block_on(terminal_event(&ambiguous, request(&profile(), true)));
    let ModelEvent::ResponseFailed(failure) = event else { panic!("failure") };
    assert_eq!(failure.category(), FailureCategory::AmbiguousAcceptance);
    assert_eq!(state.captures().len(), 1);
}

#[test]
fn explicit_rate_rejection_retries_and_authentication_does_not() {
    let rate = response(
        429,
        &[("retry-after", "0")],
        vec![crate::test_support::fixture("rate_error.json")],
    );
    let success = response(
        200,
        &[("content-type", "text/event-stream")],
        vec![crate::test_support::fixture("text.sse")],
    );
    let (retrying, state, _) = client(vec![Ok(rate), Ok(success)], 2, Vec::new());
    let event = block_on(terminal_event(&retrying, request(&profile(), true)));
    assert!(matches!(event, ModelEvent::ResponseCompleted));
    assert_eq!(state.captures().len(), 2);

    let auth = response(401, &[], vec![crate::test_support::fixture("auth_error.json")]);
    let (client, state, _) = client(vec![Ok(auth)], 2, Vec::new());
    let event = block_on(terminal_event(&client, request(&profile(), true)));
    let ModelEvent::ResponseFailed(failure) = event else { panic!("failure") };
    assert_eq!(failure.category(), FailureCategory::Authentication);
    assert_eq!(failure.http_status(), Some(401));
    assert_eq!(
        failure.response_id().map(peritus_model_protocol::ResponseId::expose_for_wire),
        Some("req_auth")
    );
    assert_eq!(state.captures().len(), 1);
    assert!(!format!("{failure:?}").contains("credential rejected"));
}

#[test]
fn http_success_with_a_non_stream_content_type_fails_as_malformed() {
    let json = response(200, &[("content-type", "application/json")], vec![b"{}".to_vec()]);
    let (client, state, _) = client(vec![Ok(json)], 1, Vec::new());
    let event = block_on(terminal_event(&client, request(&profile(), true)));
    let ModelEvent::ResponseFailed(failure) = event else { panic!("failure") };
    assert_eq!(failure.category(), FailureCategory::MalformedPayload);
    assert_eq!(state.captures().len(), 1);
}

#[test]
fn opencode_messages_uses_exact_gateway_operation_and_native_stream() {
    for endpoint in
        ["https://opencode.ai/zen/v1/messages", "https://opencode.ai/zen/go/v1/messages"]
    {
        let state = TransportState::with_responses(vec![Ok(response(
            200,
            &[("content-type", "text/event-stream")],
            vec![crate::test_support::fixture("text.sse")],
        ))]);
        let config = crate::test_support::config_at(endpoint, 1, Vec::new())
            .with_opencode_gateway()
            .expect("gateway");
        let client = AnthropicClient::with_transport(
            config,
            Box::new(TestCredentials::default()),
            Box::new(TestTransport(std::sync::Arc::clone(&state))),
        );
        assert!(matches!(
            block_on(terminal_event(&client, request(&profile(), true))),
            ModelEvent::ResponseCompleted
        ));
        let captures = state.captures();
        assert_eq!(captures[0].endpoint, endpoint);
        let wire: serde_json::Value = serde_json::from_slice(&captures[0].body).expect("wire");
        assert_eq!(wire["stream"], true);
        assert!(wire.get("max_tokens").is_some());
        assert!(captures[0].headers.iter().any(|header| header.0 == "x-api-key" && header.1));
    }
    assert!(config(1, Vec::new()).with_opencode_gateway().is_err());
}
