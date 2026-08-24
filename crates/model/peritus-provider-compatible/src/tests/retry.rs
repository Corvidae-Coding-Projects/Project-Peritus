use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use peritus_model_protocol::{Capability, FailureCategory, ModelEvent};
use peritus_provider_core::{
    BoxFuture, CancellationToken, Endpoint, HttpRequest, HttpTransport, ModelProvider,
    ProviderCoreError, RetryPolicy,
};
use peritus_test_support::{
    ExpectedHttpRequest, FakeHttpExchangeScript, FakeHttpFault, FakeHttpHeader, FakeHttpLimits,
    FakeHttpSequenceServer, HeaderMatchMode, ScriptedHttpResponse,
};

use super::support::{
    StaticCredential, block_on, credential_reference, fixture, minimal_request, responses_profile,
};
use crate::{
    CompatibleAuth, CompatibleClient, CompatibleConfig, CompatibleProfile, CompatibleRetryStatuses,
};

#[test]
fn explicitly_mapped_429_and_5xx_rejections_retry_through_real_http() {
    for status in [429, 503] {
        block_on(async move {
            let provider = responses_profile(&[Capability::Streaming, Capability::UsageDetail]);
            let profile = CompatibleProfile::responses(provider.clone()).expect("profile");
            let request = minimal_request(&provider);
            let body = crate::request::encode(&profile, &request).expect("body");
            let limits = FakeHttpLimits::default();
            let scripts = vec![
                FakeHttpExchangeScript::new(
                    expectation(body.clone(), limits),
                    response(
                        status,
                        if status == 429 {
                            vec![FakeHttpHeader::new("retry-after", "0").expect("header")]
                        } else {
                            Vec::new()
                        },
                        fixture(if status == 429 {
                            "rate-error.json"
                        } else {
                            "transient-error.json"
                        }),
                        limits,
                    ),
                ),
                FakeHttpExchangeScript::new(
                    expectation(body.clone(), limits),
                    response(
                        200,
                        vec![
                            FakeHttpHeader::new("content-type", "text/event-stream")
                                .expect("header"),
                        ],
                        fixture("responses-success.sse"),
                        limits,
                    ),
                ),
            ];
            let server = FakeHttpSequenceServer::start(scripts, limits).expect("server");
            let endpoint =
                Endpoint::new(format!("{}/compatible", server.base_url().trim_end_matches('/')))
                    .expect("endpoint");
            let policy = RetryPolicy::new(
                2,
                [
                    Duration::from_millis(1),
                    Duration::from_millis(1),
                    Duration::from_millis(1),
                    Duration::from_secs(1),
                ],
                1024 * 1024,
            )
            .expect("policy");
            let config = CompatibleConfig::new(
                endpoint,
                CompatibleAuth::bearer(credential_reference()).expect("auth"),
            )
            .expect("config")
            .with_retry_policy(policy)
            .with_retry_statuses(
                CompatibleRetryStatuses::none().with_rate_limited().with_server_errors(),
            );
            let credentials = Arc::new(StaticCredential::new());
            let client =
                CompatibleClient::new(config, profile, credentials.clone()).expect("client");
            let mut stream = client.start(request, CancellationToken::new()).await.expect("start");
            let mut completed = false;
            while let Some(event) = stream.pull().await.expect("pull") {
                completed |= matches!(event.event(), ModelEvent::ResponseCompleted);
            }
            assert!(completed);
            assert_eq!(credentials.resolutions(), 2);
            let exchanges = server.finish().expect("exchanges");
            assert_eq!(exchanges.len(), 2);
            assert!(exchanges.iter().all(|value| value.request().matched()));
        });
    }
}

#[derive(Default)]
struct AmbiguousTransport {
    requests: AtomicU64,
}

impl HttpTransport for AmbiguousTransport {
    fn send<'a>(
        &'a self,
        _request: HttpRequest,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<peritus_provider_core::HttpResponse, ProviderCoreError>> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(ProviderCoreError::transport("test", "ambiguous submission")) })
    }
}

#[test]
fn ambiguous_submission_never_blindly_retries() {
    block_on(async {
        let provider = responses_profile(&[Capability::Streaming]);
        let profile = CompatibleProfile::responses(provider.clone()).expect("profile");
        let request = minimal_request(&provider);
        let endpoint = Endpoint::new("http://127.0.0.1:9/compatible".to_owned()).expect("endpoint");
        let config = CompatibleConfig::new(
            endpoint,
            CompatibleAuth::bearer(credential_reference()).expect("auth"),
        )
        .expect("config")
        .with_retry_statuses(
            CompatibleRetryStatuses::none().with_rate_limited().with_server_errors(),
        );
        let transport = Arc::new(AmbiguousTransport::default());
        let credentials = Arc::new(StaticCredential::new());
        let client = CompatibleClient::with_transport(
            config,
            profile,
            credentials.clone(),
            transport.clone(),
        );
        let mut stream = client.start(request, CancellationToken::new()).await.expect("stream");
        let event = stream.pull().await.expect("pull").expect("failure");
        assert!(matches!(
            event.event(),
            ModelEvent::ResponseFailed(failure)
                if failure.category() == FailureCategory::AmbiguousAcceptance
        ));
        assert_eq!(transport.requests.load(Ordering::SeqCst), 1);
        assert_eq!(credentials.resolutions(), 1);
    });
}

fn expectation(body: Vec<u8>, limits: FakeHttpLimits) -> ExpectedHttpRequest {
    ExpectedHttpRequest::new("POST", "/compatible", Vec::new(), body, limits)
        .expect("expectation")
        .header_match_mode(HeaderMatchMode::AllowAdditional)
}

fn response(
    status: u16,
    headers: Vec<FakeHttpHeader>,
    body: Vec<u8>,
    limits: FakeHttpLimits,
) -> ScriptedHttpResponse {
    ScriptedHttpResponse::new(status, headers, vec![body], FakeHttpFault::Complete, None, limits)
        .expect("response")
}
