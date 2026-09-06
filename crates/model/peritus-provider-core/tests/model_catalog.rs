//! Discovery uses metadata-only requests, bounded token pagination and no substitute models.

#[path = "support/runtime.rs"]
mod runtime;

use peritus_provider_core::catalog::{CatalogDialect, DiscoveredModel, discover_http_models};
use peritus_provider_core::{
    BoxFuture, CancellationToken, Endpoint, HttpHeaders, HttpLimits, HttpMethod, HttpRequest,
    HttpResponse, HttpTransport, MemoryByteStream, ProviderCoreError, StatusCode,
};
use std::{collections::VecDeque, sync::Mutex};

struct CatalogTransport {
    pages: Mutex<VecDeque<(u16, &'static str)>>,
    urls: Mutex<Vec<String>>,
}
fn limits() -> HttpLimits {
    HttpLimits::new([16, 4096, 1024, 65536, 65536]).expect("limits")
}
impl HttpTransport for CatalogTransport {
    fn send<'a>(
        &'a self,
        request: HttpRequest,
        _: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<HttpResponse, ProviderCoreError>> {
        Box::pin(async move {
            assert_eq!(request.method(), HttpMethod::Get);
            assert!(request.body().is_empty(), "discovery must never send inference");
            self.urls.lock().expect("urls").push(request.endpoint().as_str().to_owned());
            let (status, page) =
                self.pages.lock().expect("pages").pop_front().expect("metadata page");
            HttpResponse::new(
                StatusCode::new(status).expect("status"),
                HttpHeaders::empty(),
                Box::new(MemoryByteStream::new(vec![page.as_bytes().to_vec()], limits())?),
                limits(),
            )
        })
    }
}
fn transport(pages: Vec<(u16, &'static str)>) -> CatalogTransport {
    CatalogTransport { pages: Mutex::new(pages.into()), urls: Mutex::new(Vec::new()) }
}
async fn discover(
    transport: &CatalogTransport,
    dialect: CatalogDialect,
) -> Result<Vec<DiscoveredModel>, ProviderCoreError> {
    discover_http_models(
        transport,
        &Endpoint::new("https://catalog.example/v1/models".to_owned()).expect("endpoint"),
        dialect,
        &|| Ok(HttpHeaders::empty()),
        limits(),
        &CancellationToken::new(),
    )
    .await
}

#[test]
fn anthropic_pagination_keeps_origin_and_preserves_exact_ids() {
    runtime::block_on(async {
        let transport = transport(vec![
            (
                200,
                r#"{"data":[{"id":"vendor-arbitrary-a","display_name":"First"}],"has_more":true,"last_id":"https://evil.example/?secret=x"}"#,
            ),
            (200, r#"{"data":[{"id":"vendor-arbitrary-b"}],"has_more":false}"#),
        ]);
        let models = discover(&transport, CatalogDialect::Anthropic).await.expect("catalog");
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].id.as_str(), "vendor-arbitrary-a");
        assert_eq!(models[0].tools, None);
        let urls = transport.urls.lock().expect("urls").clone();
        assert_eq!(urls.len(), 2);
        assert!(
            urls[1].starts_with(
                "https://catalog.example/v1/models?after_id=https%3A%2F%2Fevil.example"
            )
        );
    });
}

#[test]
fn google_pagination_preserves_explicit_generation_metadata() {
    runtime::block_on(async {
        let transport = transport(vec![
            (
                200,
                r#"{"models":[{"name":"models/vendor-new","displayName":"New","inputTokenLimit":12345,"supportedGenerationMethods":["generateContent"]}],"nextPageToken":"next"}"#,
            ),
            (200, r#"{"models":[]}"#),
        ]);
        let models = discover(&transport, CatalogDialect::Google).await.expect("catalog");
        assert_eq!(models[0].id.as_str(), "vendor-new");
        assert_eq!(models[0].input_tokens, Some(12345));
        assert!(transport.urls.lock().expect("urls")[1].ends_with("?pageToken=next"));
    });
}

#[test]
fn unavailable_or_malformed_catalogs_never_return_a_fallback_or_leak_bodies() {
    runtime::block_on(async {
        for (status, body) in [
            (401, "secret-credential-fixture"),
            (200, "not-json"),
            (200, r#"{"data":[{"id":"bad\u001bname"}]}"#),
        ] {
            let transport = transport(vec![(status, body)]);
            let error =
                discover(&transport, CatalogDialect::OpenAi).await.expect_err("fail closed");
            assert!(!error.to_string().contains("secret-credential-fixture"));
        }
        let transport = transport(vec![(200, r#"{"data":[]}"#)]);
        assert!(
            discover(&transport, CatalogDialect::OpenAi).await.expect("empty catalog").is_empty()
        );
    });
}

#[test]
fn repeated_pagination_and_conflicting_duplicates_are_rejected() {
    runtime::block_on(async {
        for pages in [
            vec![(200, r#"{"data":[],"has_more":true,"last_id":"same"}"#); 2],
            vec![(
                200,
                r#"{"data":[{"id":"same","display_name":"A"},{"id":"same","display_name":"B"}]}"#,
            )],
        ] {
            assert!(discover(&transport(pages), CatalogDialect::Anthropic).await.is_err());
        }
    });
}
