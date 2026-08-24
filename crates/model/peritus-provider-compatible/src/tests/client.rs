use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use peritus_model_protocol::{Capability, ModelEvent, RateLimitDimension};
use peritus_provider_core::{
    BoxFuture, CancellationToken, Endpoint, HeaderName, HttpRequest, HttpTransport, ModelProvider,
    ProviderCoreError, ProviderCoreErrorKind,
};
use peritus_test_support::{
    ExpectedHttpRequest, FakeHttpFault, FakeHttpHeader, FakeHttpLimits, FakeHttpServer,
    HeaderMatchMode, ScriptedHttpResponse,
};

use super::support::{
    StaticCredential, block_on, chat_profile, credential_reference, fixture, minimal_request,
    request_with_capabilities, responses_profile,
};
use crate::{
    CompatibleAuth, CompatibleClient, CompatibleConfig, CompatibleProfile, CompatibleRateHeaders,
    CompatibleResetUnit, CompatibleResponseHeaders,
};

#[test]
fn production_client_sends_exact_endpoint_and_bearer_header() {
    block_on(async {
        let provider = responses_profile(&[Capability::Streaming, Capability::UsageDetail]);
        let profile = CompatibleProfile::responses(provider.clone()).expect("profile");
        let request = minimal_request(&provider);
        let body = crate::request::encode(&profile, &request).expect("body");
        let limits = FakeHttpLimits::default();
        let expected =
            ExpectedHttpRequest::new("POST", "/custom/responses", Vec::new(), body, limits)
                .expect("expected")
                .header_match_mode(HeaderMatchMode::AllowAdditional);
        let response = ScriptedHttpResponse::new(
            200,
            vec![
                FakeHttpHeader::new("content-type", "text/event-stream").expect("header"),
                FakeHttpHeader::new("x-compatible-request-id", "request-provider")
                    .expect("request ID"),
                FakeHttpHeader::new("x-compatible-limit", "100").expect("limit"),
                FakeHttpHeader::new("x-compatible-remaining", "99").expect("remaining"),
                FakeHttpHeader::new("x-compatible-reset-ms", "250").expect("reset"),
            ],
            vec![fixture("responses-success.sse")],
            FakeHttpFault::Complete,
            None,
            limits,
        )
        .expect("response");
        let server = FakeHttpServer::start(expected, response, limits).expect("server");
        let endpoint =
            Endpoint::new(format!("{}/custom/responses", server.base_url().trim_end_matches('/')))
                .expect("endpoint");
        let rate_headers = CompatibleRateHeaders::new(
            HeaderName::new("x-compatible-limit".to_owned()).expect("limit name"),
            HeaderName::new("x-compatible-remaining".to_owned()).expect("remaining name"),
            HeaderName::new("x-compatible-reset-ms".to_owned()).expect("reset name"),
            RateLimitDimension::Requests,
            CompatibleResetUnit::Milliseconds,
        )
        .expect("rate mapping");
        let response_headers = CompatibleResponseHeaders::none()
            .with_request_id(
                HeaderName::new("x-compatible-request-id".to_owned()).expect("request ID name"),
            )
            .expect("request ID mapping")
            .with_rate_limit(rate_headers);
        let config = CompatibleConfig::new(
            endpoint,
            CompatibleAuth::bearer(credential_reference()).expect("auth"),
        )
        .expect("config")
        .with_response_headers(response_headers);
        let credentials = Arc::new(StaticCredential::new());
        let client = CompatibleClient::new(config, profile, credentials.clone()).expect("client");
        let mut stream = client.start(request, CancellationToken::new()).await.expect("start");
        let mut completed = false;
        let mut request_id = false;
        let mut rate_limit = false;
        while let Some(event) = stream.pull().await.expect("pull") {
            completed |= matches!(event.event(), ModelEvent::ResponseCompleted);
            request_id |= matches!(
                event.event(),
                ModelEvent::ProviderEvent(extension)
                    if extension.name().as_str() == "compatible.request_id"
                        && extension.value().canonical_bytes() == br#""request-provider""#
            );
            rate_limit |= matches!(event.event(), ModelEvent::RateLimit(_));
        }
        assert!(completed);
        assert!(request_id);
        assert!(rate_limit);
        assert_eq!(credentials.resolutions(), 1);
        let exchange = server.finish().expect("exchange");
        assert!(exchange.request().matched());
        assert!(
            exchange
                .request()
                .headers()
                .iter()
                .any(|header| { header.name() == "authorization" && header.is_sensitive() })
        );
    });
}

#[test]
fn production_client_uses_the_separately_validated_chat_dialect() {
    block_on(async {
        let provider = chat_profile(&[Capability::Streaming, Capability::UsageDetail]);
        let profile = CompatibleProfile::chat_completions(provider.clone()).expect("profile");
        let request = minimal_request(&provider);
        let body = crate::request::encode(&profile, &request).expect("body");
        let limits = FakeHttpLimits::default();
        let expected = ExpectedHttpRequest::new("POST", "/custom/chat", Vec::new(), body, limits)
            .expect("expected")
            .header_match_mode(HeaderMatchMode::AllowAdditional);
        let response = ScriptedHttpResponse::new(
            200,
            vec![FakeHttpHeader::new("content-type", "text/event-stream").expect("header")],
            vec![fixture("chat-success.sse")],
            FakeHttpFault::Complete,
            None,
            limits,
        )
        .expect("response");
        let server = FakeHttpServer::start(expected, response, limits).expect("server");
        let endpoint =
            Endpoint::new(format!("{}/custom/chat", server.base_url().trim_end_matches('/')))
                .expect("endpoint");
        let config = CompatibleConfig::new(
            endpoint,
            CompatibleAuth::bearer(credential_reference()).expect("auth"),
        )
        .expect("config");
        let client = CompatibleClient::new(config, profile, Arc::new(StaticCredential::new()))
            .expect("client");
        let mut stream = client.start(request, CancellationToken::new()).await.expect("start");
        let mut completed = false;
        while let Some(event) = stream.pull().await.expect("pull") {
            completed |= matches!(event.event(), ModelEvent::ResponseCompleted);
        }
        assert!(completed);
        assert!(server.finish().expect("exchange").request().matched());
    });
}

struct CountingTransport(AtomicU64);

impl HttpTransport for CountingTransport {
    fn send<'a>(
        &'a self,
        _request: HttpRequest,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<peritus_provider_core::HttpResponse, ProviderCoreError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(ProviderCoreError::transport("test", "unexpected transport")) })
    }
}

#[test]
fn unsupported_request_rejects_before_credentials_and_transport() {
    block_on(async {
        let provider = responses_profile(&[Capability::Streaming]);
        let profile = CompatibleProfile::responses(provider.clone()).expect("profile");
        let request = request_with_capabilities(&provider, &[]);
        let endpoint = Endpoint::new("http://127.0.0.1:9/responses".to_owned()).expect("endpoint");
        let config = CompatibleConfig::new(
            endpoint,
            CompatibleAuth::bearer(credential_reference()).expect("auth"),
        )
        .expect("config");
        let credentials = Arc::new(StaticCredential::new());
        let transport = Arc::new(CountingTransport(AtomicU64::new(0)));
        let client = CompatibleClient::with_transport(
            config,
            profile,
            credentials.clone(),
            transport.clone(),
        );
        let error = client.start(request, CancellationToken::new()).await.expect_err("rejection");
        assert_eq!(error.kind(), ProviderCoreErrorKind::InvalidRequest);
        assert_eq!(credentials.resolutions(), 0);
        assert_eq!(transport.0.load(Ordering::SeqCst), 0);
    });
}

#[test]
fn explicitly_mapped_request_identity_is_preserved_on_http_failure() {
    block_on(async {
        let provider = responses_profile(&[Capability::Streaming]);
        let profile = CompatibleProfile::responses(provider.clone()).expect("profile");
        let request = minimal_request(&provider);
        let body = crate::request::encode(&profile, &request).expect("body");
        let limits = FakeHttpLimits::default();
        let expected = ExpectedHttpRequest::new("POST", "/failure", Vec::new(), body, limits)
            .expect("expected")
            .header_match_mode(HeaderMatchMode::AllowAdditional);
        let response = ScriptedHttpResponse::new(
            401,
            vec![FakeHttpHeader::new("x-trace-id", "trace-safe-1").expect("trace")],
            vec![fixture("auth-error.json")],
            FakeHttpFault::Complete,
            None,
            limits,
        )
        .expect("response");
        let server = FakeHttpServer::start(expected, response, limits).expect("server");
        let endpoint =
            Endpoint::new(format!("{}/failure", server.base_url().trim_end_matches('/')))
                .expect("endpoint");
        let mappings = CompatibleResponseHeaders::none()
            .with_request_id(HeaderName::new("x-trace-id".to_owned()).expect("name"))
            .expect("mapping");
        let config = CompatibleConfig::new(
            endpoint,
            CompatibleAuth::bearer(credential_reference()).expect("auth"),
        )
        .expect("config")
        .with_response_headers(mappings);
        let client = CompatibleClient::new(config, profile, Arc::new(StaticCredential::new()))
            .expect("client");
        let mut stream = client.start(request, CancellationToken::new()).await.expect("start");
        let terminal = stream.pull().await.expect("pull").expect("terminal");
        assert!(matches!(
            terminal.event(),
            ModelEvent::ResponseFailed(failure)
                if failure.response_id().is_some_and(|identity| {
                    identity.expose_for_wire() == "trace-safe-1"
                })
        ));
        assert!(server.finish().expect("exchange").request().matched());
    });
}
