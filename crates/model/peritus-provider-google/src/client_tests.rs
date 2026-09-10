//! Authenticated stable-v1 submission and retry-boundary tests.

use std::sync::atomic::Ordering;

use peritus_model_protocol::{FailureCategory, ModelEvent, WireDialect};
use peritus_provider_core::{
    CancellationToken, HttpMethod, ModelProvider, ProviderCoreError, ProviderCoreErrorKind,
};

use super::GoogleClient;
use crate::test_support::{
    TestCredentials, TestTransport, TransportState, block_on, config, fixture, profile, request,
    response,
};

fn client(
    dialect: WireDialect,
    responses: Vec<Result<peritus_provider_core::HttpResponse, ProviderCoreError>>,
    attempts: u32,
) -> (GoogleClient, std::sync::Arc<TransportState>, std::sync::Arc<std::sync::atomic::AtomicUsize>)
{
    let state = TransportState::with_responses(responses);
    let credentials = TestCredentials::default();
    let resolutions = credentials.counter();
    let client = GoogleClient::with_transport(
        config(dialect, attempts),
        Box::new(credentials),
        Box::new(TestTransport(std::sync::Arc::clone(&state))),
    );
    (client, state, resolutions)
}

async fn terminal_event(
    client: &GoogleClient,
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

fn success(dialect: WireDialect) -> peritus_provider_core::HttpResponse {
    let fixture_name = match dialect {
        WireDialect::GeminiInteractionsV1 => "interactions_success.sse",
        _ => "generate_success.sse",
    };
    response(
        200,
        &[("content-type", "text/event-stream; charset=utf-8")],
        vec![fixture(fixture_name)],
    )
}

#[test]
fn sends_header_auth_and_exact_stable_v1_routes_for_both_dialects() {
    for dialect in [WireDialect::GeminiInteractionsV1, WireDialect::GeminiGenerateContentV1] {
        let (client, state, resolutions) = client(dialect, vec![Ok(success(dialect))], 1);
        let terminal = block_on(terminal_event(&client, request(&profile(dialect), true)));
        assert!(matches!(terminal, ModelEvent::ResponseCompleted));
        assert_eq!(resolutions.load(Ordering::SeqCst), 1);
        let captured = state.captures();
        assert_eq!(captured[0].method, HttpMethod::Post);
        assert!(!captured[0].body.is_empty());
        assert!(captured[0].endpoint.contains("/v1/"));
        assert!(!captured[0].endpoint.contains("v1beta"));
        let key = captured[0]
            .headers
            .iter()
            .find(|header| header.0 == "x-goog-api-key")
            .expect("API key");
        assert!(key.1);
        assert!(key.2.is_none());
        assert!(!format!("{client:?} {captured:?}").contains("google-secret-marker"));
    }
}

#[test]
fn missing_streaming_stops_before_credentials_and_transport() {
    let dialect = WireDialect::GeminiInteractionsV1;
    let (client, state, resolutions) = client(dialect, Vec::new(), 1);
    let result =
        block_on(client.start(request(&profile(dialect), false), CancellationToken::new()));
    let Err(error) = result else { panic!("streaming not negotiated") };
    assert_eq!(error.kind(), ProviderCoreErrorKind::InvalidRequest);
    assert_eq!(resolutions.load(Ordering::SeqCst), 0);
    assert!(state.captures().is_empty());
}

#[test]
fn connect_retries_but_maybe_sent_transport_does_not() {
    let dialect = WireDialect::GeminiInteractionsV1;
    let (retrying, state, resolutions) = client(
        dialect,
        vec![
            Err(ProviderCoreError::connect("test", "connection failed before submission")),
            Ok(success(dialect)),
        ],
        2,
    );
    let event = block_on(terminal_event(&retrying, request(&profile(dialect), true)));
    assert!(matches!(event, ModelEvent::ResponseCompleted));
    assert_eq!(state.captures().len(), 2);
    assert_eq!(resolutions.load(Ordering::SeqCst), 2);

    let (ambiguous, state, _) = client(
        dialect,
        vec![Err(ProviderCoreError::transport("test", "submission outcome is unknown"))],
        2,
    );
    let event = block_on(terminal_event(&ambiguous, request(&profile(dialect), true)));
    let ModelEvent::ResponseFailed(failure) = event else { panic!("failure") };
    assert_eq!(failure.category(), FailureCategory::AmbiguousAcceptance);
    assert_eq!(state.captures().len(), 1);
}

#[test]
fn explicit_rate_rejection_retries_but_authentication_does_not() {
    let dialect = WireDialect::GeminiInteractionsV1;
    let rate = response(429, &[("retry-after", "0")], vec![fixture("rate_error.json")]);
    let (retrying, state, _) = client(dialect, vec![Ok(rate), Ok(success(dialect))], 2);
    let event = block_on(terminal_event(&retrying, request(&profile(dialect), true)));
    assert!(matches!(event, ModelEvent::ResponseCompleted));
    assert_eq!(state.captures().len(), 2);

    let auth = response(
        401,
        &[("x-goog-request-id", "google-request-401")],
        vec![fixture("auth_error.json")],
    );
    let (client, state, _) = client(dialect, vec![Ok(auth)], 2);
    let event = block_on(terminal_event(&client, request(&profile(dialect), true)));
    let ModelEvent::ResponseFailed(failure) = event else { panic!("failure") };
    assert_eq!(failure.category(), FailureCategory::Authentication);
    assert_eq!(failure.http_status(), Some(401));
    assert_eq!(
        failure.response_id().map(peritus_model_protocol::ResponseId::expose_for_wire),
        Some("google-request-401")
    );
    assert_eq!(state.captures().len(), 1);
    assert!(!format!("{failure:?}").contains("credential rejected"));
}

#[test]
fn successful_non_sse_shape_is_malformed() {
    let dialect = WireDialect::GeminiInteractionsV1;
    let json = response(200, &[("content-type", "application/json")], vec![b"{}".to_vec()]);
    let (client, state, _) = client(dialect, vec![Ok(json)], 1);
    let event = block_on(terminal_event(&client, request(&profile(dialect), true)));
    let ModelEvent::ResponseFailed(failure) = event else { panic!("failure") };
    assert_eq!(failure.category(), FailureCategory::MalformedPayload);
    assert_eq!(state.captures().len(), 1);
}

#[test]
fn generate_numeric_error_envelope_remains_rate_limited_without_quota_evidence() {
    let dialect = WireDialect::GeminiGenerateContentV1;
    let rate = response(429, &[], vec![fixture("generate_rate_error.json")]);
    let (client, state, _) = client(dialect, vec![Ok(rate)], 1);
    let event = block_on(terminal_event(&client, request(&profile(dialect), true)));
    let ModelEvent::ResponseFailed(failure) = event else { panic!("failure") };
    assert_eq!(failure.category(), FailureCategory::RateLimited);
    assert_eq!(failure.http_status(), Some(429));
    assert_eq!(state.captures().len(), 1);
    assert!(!format!("{failure:?}").contains("Resource exhausted"));
}

#[test]
fn opencode_generate_content_preserves_prefix_and_native_stream() {
    use peritus_provider_core::{CredentialReference, Endpoint, FramingLimits, HttpLimits};
    let dialect = WireDialect::GeminiGenerateContentV1;
    for endpoint in ["https://opencode.ai/zen/", "https://opencode.ai/zen/go/"] {
        let selected = profile(dialect);
        let expected = format!(
            "{endpoint}v1/models/{}:streamGenerateContent?alt=sse",
            selected.model().as_str()
        );
        let config = crate::GoogleConfig::opencode_gateway(
            Endpoint::new(endpoint.to_owned()).expect("endpoint"),
            CredentialReference::new("fixture".to_owned()).expect("reference"),
            selected.clone(),
            HttpLimits::PRODUCTION,
            FramingLimits::PRODUCTION,
            config(dialect, 1).retry_policy(),
        )
        .expect("gateway");
        let state = TransportState::with_responses(vec![Ok(success(dialect))]);
        let client = GoogleClient::with_transport(
            config,
            Box::new(TestCredentials::default()),
            Box::new(TestTransport(std::sync::Arc::clone(&state))),
        );
        assert!(matches!(
            block_on(terminal_event(&client, request(&selected, true))),
            ModelEvent::ResponseCompleted
        ));
        let captures = state.captures();
        assert_eq!(captures[0].endpoint, expected);
        let wire: serde_json::Value = serde_json::from_slice(&captures[0].body).expect("wire");
        assert!(wire.get("contents").is_some());
        assert!(captures[0].headers.iter().any(|header| header.0 == "x-goog-api-key" && header.1));
    }
}

#[test]
fn opencode_google_connection_check_uses_portable_schemas_and_matching_function_results() {
    use peritus_provider_core::{
        CredentialReference, Endpoint, FramingLimits, HttpLimits,
        connection::verify_provider_connection,
    };
    let dialect = WireDialect::GeminiGenerateContentV1;
    let config = crate::GoogleConfig::opencode_gateway(
        Endpoint::new("https://opencode.ai/zen/".to_owned()).expect("endpoint"),
        CredentialReference::new("fixture".to_owned()).expect("reference"),
        profile(dialect),
        HttpLimits::PRODUCTION,
        FramingLimits::PRODUCTION,
        config(dialect, 1).retry_policy(),
    )
    .expect("gateway");
    let tools = String::from_utf8(fixture("generate_tool_thinking.sse"))
        .expect("fixture")
        .replace("weather", "peritus_connection_check")
        .replace(r#"{"city":"Paris"}"#, "{}")
        .into_bytes();
    let result = String::from_utf8(fixture("generate_success.sse"))
        .expect("fixture")
        .replace("héllo", "peritus-connection-ok")
        .into_bytes();
    let state = TransportState::with_responses(vec![
        Ok(success(dialect)),
        Ok(response(200, &[("content-type", "text/event-stream")], vec![tools])),
        Ok(response(200, &[("content-type", "text/event-stream")], vec![result])),
    ]);
    let client = GoogleClient::with_transport(
        config,
        Box::new(TestCredentials::default()),
        Box::new(TestTransport(std::sync::Arc::clone(&state))),
    );
    block_on(verify_provider_connection(&client, CancellationToken::new()))
        .expect("all connection stages");
    let captures = state.captures();
    let tool: serde_json::Value = serde_json::from_slice(&captures[1].body).expect("tool request");
    assert_eq!(
        tool["tools"][0]["functionDeclarations"][0]["parametersJsonSchema"]["type"],
        "object"
    );
    let result: serde_json::Value =
        serde_json::from_slice(&captures[2].body).expect("result request");
    let response = &result["contents"][2]["parts"][0]["functionResponse"];
    assert_eq!(response["name"], "peritus_connection_check");
    assert_eq!(response["id"], "call-peritus_connection_check");
    assert_eq!(response["response"]["token"], "peritus-connection-ok");
}
