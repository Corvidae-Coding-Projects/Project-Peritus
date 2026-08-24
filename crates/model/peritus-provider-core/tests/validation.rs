//! Endpoint, HTTP value, credential, and diagnostic validation tests.

use std::sync::Arc;

use peritus_provider_core::{
    Credential, CredentialReference, CredentialSource, Diagnostic, DiagnosticValue, Endpoint,
    Header, HeaderName, HttpHeaders, HttpLimits, HttpMethod, HttpRequest, ProviderCoreError,
    ProviderCoreErrorKind, RedactedValue, StatusCode, TransportPhase,
};

#[test]
fn endpoint_accepts_safe_http_and_https_urls() {
    let endpoint = Endpoint::new("https://api.example.test/v1/models?region=us".to_owned())
        .expect("safe endpoint");
    assert_eq!(endpoint.as_str(), "https://api.example.test/v1/models?region=us");
    assert_eq!(
        endpoint.with_path("/v1/responses").expect("safe replacement").as_str(),
        "https://api.example.test/v1/responses"
    );
    Endpoint::new("http://127.0.0.1:8080/test".to_owned()).expect("loopback test endpoint");
}

#[test]
fn endpoint_rejects_credentials_fragments_traversal_and_secret_queries() {
    for unsafe_endpoint in [
        "ftp://api.example.test/v1",
        "https://user:password@api.example.test/v1",
        "https://api.example.test/v1#fragment",
        "https://api.example.test/v1/../secret",
        "https://api.example.test/v1/%2e%2e/secret",
        "https://api.example.test/v1/%5csecret",
        "https://api.example.test/v1?api_key=canary",
        "https://api.example.test/v1?apikey=canary",
        "https://api.example.test/v1?access-token=canary",
        "https://api.example.test/v1?authorization=canary",
        "https://api.example.test/v1?x-amz-signature=canary",
    ] {
        let error = Endpoint::new(unsafe_endpoint.to_owned()).expect_err(unsafe_endpoint);
        assert_eq!(error.kind(), ProviderCoreErrorKind::InvalidEndpoint);
        assert!(!error.to_string().contains("canary"));
    }
    let endpoint = Endpoint::new("https://api.example.test/v1".to_owned()).expect("base endpoint");
    endpoint
        .with_path("/v1/../secret")
        .expect_err("traversal must remain visible before URL normalization");
}

#[test]
fn headers_and_bodies_are_checked_and_debug_redacted() {
    let limits = HttpLimits::new([4, 128, 8, 16, 8]).expect("test limits");
    let secret_name = HeaderName::new("Authorization".to_owned()).expect("header name");
    let secret = Credential::new(b"unique-credential-canary".to_vec()).expect("credential");
    let header = secret.into_header(secret_name, Some("Bearer ")).expect("credential header");
    assert!(header.value().is_sensitive());
    assert!(header.value().nonsensitive_bytes().is_none());
    assert!(!format!("{header:?}").contains("unique-credential-canary"));
    let headers = HttpHeaders::new(vec![header], limits).expect("bounded headers");
    let request = HttpRequest::new(
        HttpMethod::Post,
        Endpoint::new("https://api.example.test/v1".to_owned()).expect("endpoint"),
        headers,
        b"request".to_vec(),
        limits,
    )
    .expect("bounded request");
    assert!(!format!("{request:?}").contains("unique-credential-canary"));
    assert!(!format!("{request:?}").contains("request"));

    let controlled = Header::new(
        HeaderName::new("Content-Length".to_owned()).expect("valid syntax"),
        b"1".to_vec(),
    )
    .expect("header");
    let controlled = HttpHeaders::new(vec![controlled], limits).expect("bounded headers");
    let error = HttpRequest::new(
        HttpMethod::Post,
        Endpoint::new("https://api.example.test/v1".to_owned()).expect("endpoint"),
        controlled,
        Vec::new(),
        limits,
    )
    .expect_err("caller-controlled content length must fail");
    assert_eq!(error.kind(), ProviderCoreErrorKind::InvalidHttp);

    let oversized = HttpRequest::new(
        HttpMethod::Post,
        Endpoint::new("https://api.example.test/v1".to_owned()).expect("endpoint"),
        HttpHeaders::empty(),
        vec![0; 9],
        limits,
    )
    .expect_err("body limit");
    assert_eq!(oversized.kind(), ProviderCoreErrorKind::LimitExceeded);
}

#[derive(Debug)]
struct FixedCredentialSource {
    bytes: Arc<Vec<u8>>,
}

impl CredentialSource for FixedCredentialSource {
    fn resolve(&self, _reference: &CredentialReference) -> Result<Credential, ProviderCoreError> {
        Credential::new(self.bytes.as_ref().clone())
    }
}

#[test]
fn credential_source_and_diagnostics_do_not_format_sensitive_values() {
    let reference = CredentialReference::new("provider/production".to_owned()).expect("reference");
    let source = FixedCredentialSource { bytes: Arc::new(b"source-canary".to_vec()) };
    let credential = source.resolve(&reference).expect("resolved");
    assert_eq!(credential.len(), 13);
    assert!(!format!("{reference:?} {credential:?}").contains("canary"));

    let error = ProviderCoreError::transport("send", "connection failed");
    let diagnostic = Diagnostic::from_error(&error, TransportPhase::AwaitingHeaders)
        .with_status(StatusCode::new(503).expect("status"))
        .with_provider_request_id(
            RedactedValue::new("request-id-canary".to_owned()).expect("request id"),
        )
        .with_content_type(
            DiagnosticValue::new("application/json".to_owned()).expect("content type"),
        );
    let rendered = format!("{diagnostic:?}");
    assert!(!rendered.contains("request-id-canary"));
    assert!(rendered.contains("application/json"));
    assert_eq!(diagnostic.status().expect("status").as_u16(), 503);
}

#[test]
fn status_and_http_limit_boundaries_are_checked() {
    assert!(StatusCode::new(100).is_ok());
    assert!(StatusCode::new(599).is_ok());
    assert!(StatusCode::new(99).is_err());
    assert!(StatusCode::new(600).is_err());
    assert!(HttpLimits::new([0, 1, 1, 1, 1]).is_err());
    assert!(HttpLimits::new([1, 1, 1, 1, 2]).is_err());
}

#[test]
fn adapter_error_factories_preserve_static_categories() {
    for (error, expected_kind) in [
        (
            ProviderCoreError::connect("send", "connection failed before submission"),
            ProviderCoreErrorKind::Connect,
        ),
        (
            ProviderCoreError::invalid_request("project_request", "unsupported request option"),
            ProviderCoreErrorKind::InvalidRequest,
        ),
        (
            ProviderCoreError::limit_exceeded("decode_event", "event limit exceeded"),
            ProviderCoreErrorKind::LimitExceeded,
        ),
        (
            ProviderCoreError::malformed_stream("decode_event", "invalid provider event"),
            ProviderCoreErrorKind::MalformedStream,
        ),
        (
            ProviderCoreError::configuration("provider", "configuration is incomplete"),
            ProviderCoreErrorKind::Configuration,
        ),
    ] {
        assert_eq!(error.kind(), expected_kind);
        assert_eq!(error.code(), expected_kind.code());
        assert!(!error.operation().is_empty());
        assert!(!error.detail().is_empty());
    }
}
