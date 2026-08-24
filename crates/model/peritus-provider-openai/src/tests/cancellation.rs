use std::sync::Arc;

use peritus_model_protocol::ResponseId;
use peritus_provider_core::{
    CancellationToken, Endpoint, ModelProvider, ResponseCancellationOutcome,
};
use peritus_test_support::{
    ExpectedHttpRequest, FakeHttpFault, FakeHttpHeader, FakeHttpLimits, FakeHttpServer,
    HeaderMatchMode, ScriptedHttpResponse,
};

use super::support::{StaticCredential, block_on, credential_reference, profile_full};
use crate::{OpenAiConfig, OpenAiProvider};

#[test]
fn known_background_response_can_be_cancelled_through_the_official_endpoint() {
    block_on(async {
        let response_id = ResponseId::new("resp-background".to_owned()).expect("response id");
        let limits = FakeHttpLimits::default();
        let expected = ExpectedHttpRequest::new(
            "POST",
            "/v1/responses/resp-background/cancel",
            Vec::new(),
            Vec::new(),
            limits,
        )
        .expect("expectation")
        .header_match_mode(HeaderMatchMode::AllowAdditional);
        let response = ScriptedHttpResponse::new(
            200,
            vec![
                FakeHttpHeader::new("Content-Type", b"application/json".to_vec()).expect("header"),
            ],
            vec![br#"{"id":"resp-background","status":"cancelled"}"#.to_vec()],
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
        let provider =
            OpenAiProvider::new(config, profile_full(), credentials.clone()).expect("provider");
        provider.remember_background_for_test(response_id.clone()).expect("known response");

        let outcome = provider
            .cancel_response(&response_id, &CancellationToken::new())
            .await
            .expect("cancel response");
        assert_eq!(outcome, ResponseCancellationOutcome::Confirmed { already_terminal: false });
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
fn unknown_background_identity_is_rejected_before_credentials() {
    block_on(async {
        let credentials = Arc::new(StaticCredential::new());
        let provider = OpenAiProvider::new(
            OpenAiConfig::new(credential_reference()).expect("config"),
            profile_full(),
            credentials.clone(),
        )
        .expect("provider");
        let response_id = ResponseId::new("resp-unknown".to_owned()).expect("response id");
        assert!(provider.cancel_response(&response_id, &CancellationToken::new()).await.is_err());
        assert_eq!(credentials.resolutions(), 0);
    });
}
