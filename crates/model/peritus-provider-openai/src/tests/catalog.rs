use super::support::{
    StaticCredential, block_on, credential_reference, minimal_request, profile_minimal,
};
use crate::{OpenAiConfig, OpenAiProvider};
use peritus_provider_core::{
    BoxFuture, CancellationToken, Endpoint, HttpHeaders, HttpLimits, HttpMethod, HttpRequest,
    HttpResponse, HttpTransport, MemoryByteStream, ModelProvider, ProviderCoreError, StatusCode,
};
use std::sync::{Arc, Mutex};

struct MetadataAndSubmission(Mutex<Vec<String>>);
impl HttpTransport for MetadataAndSubmission {
    fn send<'a>(
        &'a self,
        request: HttpRequest,
        _: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<HttpResponse, ProviderCoreError>> {
        Box::pin(async move {
            assert!(request.headers().first("authorization").is_some());
            if request.method() == HttpMethod::Get {
                assert_eq!(request.endpoint().as_str(), "http://127.0.0.1:9/v1/models");
                assert!(request.body().is_empty());
                let limits = HttpLimits::new([16, 4096, 65536, 65536, 65536]).expect("limits");
                return HttpResponse::new(
                    StatusCode::new(200).expect("status"),
                    HttpHeaders::empty(),
                    Box::new(MemoryByteStream::new(
                        vec![br#"{"data":[{"id":"provider-brand-new-identifier"}]}"#.to_vec()],
                        limits,
                    )?),
                    limits,
                );
            }
            assert_eq!(request.method(), HttpMethod::Post);
            let body: serde_json::Value =
                serde_json::from_slice(request.body()).expect("actual wire body");
            self.0
                .lock()
                .expect("submissions")
                .push(body["model"].as_str().expect("actual model").to_owned());
            Err(ProviderCoreError::configuration(
                "fixture",
                "captured submission without inference",
            ))
        })
    }
}

#[test]
fn arbitrary_discovered_model_is_used_by_the_actual_rebound_adapter_request() {
    block_on(async {
        let transport = Arc::new(MetadataAndSubmission(Mutex::new(Vec::new())));
        let credentials = Arc::new(StaticCredential::new());
        let original = OpenAiProvider::with_transport(
            OpenAiConfig::for_test(
                Endpoint::new("http://127.0.0.1:9".to_owned()).expect("endpoint"),
                credential_reference(),
            )
            .expect("config"),
            profile_minimal(),
            credentials.clone(),
            transport.clone(),
        )
        .expect("provider");
        let models = original.discover_models(&CancellationToken::new()).await.expect("metadata");
        assert!(transport.0.lock().expect("submissions").is_empty());
        let selected = original.select_model(models[0].id.clone()).expect("immutable selection");
        assert_eq!(original.profile().model().as_str(), "gpt-test");
        let request = minimal_request(selected.profile());
        assert!(selected.start(request, CancellationToken::new()).await.is_err());
        assert_eq!(*transport.0.lock().expect("submissions"), ["provider-brand-new-identifier"]);
        assert_eq!(
            credentials.resolutions(),
            2,
            "one metadata request and one selected submission"
        );
    });
}
