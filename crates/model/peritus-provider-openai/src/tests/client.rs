use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use peritus_model_protocol::ModelEvent;
use peritus_provider_core::{
    BoxFuture, CancellationToken, Endpoint, HttpRequest, HttpTransport, ModelProvider,
    ProviderCoreError, ProviderCoreErrorKind,
};
use peritus_test_support::{
    ExpectedHttpRequest, FakeHttpFault, FakeHttpHeader, FakeHttpLimits, FakeHttpServer,
    HeaderMatchMode, ScriptedHttpResponse,
};

use super::support::{
    StaticCredential, block_on, credential_reference, fixture, minimal_request, profile_minimal,
    request_with_capabilities,
};
use crate::{OpenAiConfig, OpenAiProvider};

#[test]
fn production_subject_uses_bearer_auth_and_normalizes_fake_server_stream() {
    block_on(async {
        let profile = profile_minimal();
        let request = minimal_request(&profile);
        let body = crate::request::encode(&request).expect("encoded body");
        let limits = FakeHttpLimits::default();
        let expected = ExpectedHttpRequest::new("POST", "/v1/responses", Vec::new(), body, limits)
            .expect("expectation")
            .header_match_mode(HeaderMatchMode::AllowAdditional);
        let response = ScriptedHttpResponse::new(
            200,
            vec![
                FakeHttpHeader::new("Content-Type", b"text/event-stream".to_vec()).expect("header"),
                FakeHttpHeader::new("X-Request-Id", b"request-provider-visible".to_vec())
                    .expect("request id header"),
                FakeHttpHeader::new("X-RateLimit-Limit-Requests", b"100".to_vec())
                    .expect("rate limit header"),
                FakeHttpHeader::new("X-RateLimit-Remaining-Requests", b"99".to_vec())
                    .expect("rate remaining header"),
                FakeHttpHeader::new("X-RateLimit-Reset-Requests", b"1s".to_vec())
                    .expect("rate reset header"),
            ],
            fixture("success.sse").chunks(17).map(<[u8]>::to_vec).collect(),
            FakeHttpFault::Complete,
            None,
            limits,
        )
        .expect("response");
        let server = FakeHttpServer::start(expected, response, limits).expect("server");
        let config = OpenAiConfig::for_test(
            Endpoint::new(server.base_url()).expect("endpoint"),
            credential_reference(),
        )
        .expect("config");
        let credentials = Arc::new(StaticCredential::new());
        let provider = OpenAiProvider::new(config, profile, credentials.clone()).expect("provider");
        let mut stream = provider.start(request, CancellationToken::new()).await.expect("start");
        let mut completed = false;
        let mut request_id_observed = false;
        let mut rate_limit_observed = false;
        while let Some(event) = stream.pull().await.expect("pull") {
            completed |= matches!(event.event(), ModelEvent::ResponseCompleted);
            rate_limit_observed |= matches!(event.event(), ModelEvent::RateLimit(_));
            request_id_observed |= matches!(
                event.event(),
                ModelEvent::ProviderEvent(extension)
                    if extension.name().as_str() == "openai.request_id"
                        && extension.value().canonical_bytes()
                            == br#""request-provider-visible""#
            );
        }
        assert!(completed);
        assert!(request_id_observed);
        assert!(rate_limit_observed);
        assert_eq!(credentials.resolutions(), 1);
        let exchange = server.finish().expect("exchange");
        assert!(exchange.request().matched());
        assert!(
            exchange
                .request()
                .headers()
                .iter()
                .any(|header| header.name() == "authorization" && header.is_sensitive())
        );
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
fn nonnegotiated_streaming_rejects_before_credentials_and_transport() {
    block_on(async {
        let profile = profile_minimal();
        let request = request_with_capabilities(&profile, &[]);
        let credentials = Arc::new(StaticCredential::new());
        let transport = Arc::new(CountingTransport(AtomicU64::new(0)));
        let config = OpenAiConfig::for_test(
            Endpoint::new("http://127.0.0.1:9".to_owned()).expect("endpoint"),
            credential_reference(),
        )
        .expect("config");
        let provider =
            OpenAiProvider::with_transport(config, profile, credentials.clone(), transport.clone())
                .expect("provider");
        let error =
            provider.start(request, CancellationToken::new()).await.expect_err("request rejection");
        assert_eq!(error.kind(), ProviderCoreErrorKind::InvalidRequest);
        assert_eq!(credentials.resolutions(), 0);
        assert_eq!(transport.0.load(Ordering::SeqCst), 0);
    });
}
