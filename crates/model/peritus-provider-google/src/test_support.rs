//! Shared test-only fixtures and deterministic provider boundaries.

use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use peritus_model_protocol::{
    BoundedText, CachePolicy, CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance,
    ContentBlock, GenerationConfig, Message, ModelLimits, ModelName, ModelRequest,
    OutputLimitEnforcement, ParallelToolPolicy, PersistencePolicy, ProtocolLimits, ProviderName,
    ProviderProfile, ReasoningPolicy, RequestId, RequestOptions, RequestedCapabilities, ResumeKind,
    Role, StateMode, StructuredOutput, ToolChoice, WireDialect, negotiate,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, Credential, CredentialReference, CredentialSource, Endpoint,
    FramingLimits, Header, HeaderName, HttpHeaders, HttpLimits, HttpMethod, HttpRequest,
    HttpResponse, HttpTransport, MemoryByteStream, ProviderCoreError, RetryPolicy, StatusCode,
};
use peritus_types::ProviderProfileId;

use crate::GoogleConfig;

pub fn block_on<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

pub fn fixture(name: &str) -> Vec<u8> {
    let package = std::path::Path::new("fixtures/v1").join(name);
    let workspace =
        std::path::Path::new("crates/model/peritus-provider-google/fixtures/v1").join(name);
    std::fs::read(package).or_else(|_| std::fs::read(workspace)).expect("Google fixture")
}

pub fn profile(dialect: WireDialect) -> ProviderProfile {
    let supported = [
        Capability::Streaming,
        Capability::ToolCalls,
        Capability::ParallelToolCalls,
        Capability::StrictStructuredOutput,
        Capability::PromptCaching,
        Capability::ImageInput,
        Capability::AudioInput,
        Capability::DocumentInput,
        Capability::ReasoningControls,
        Capability::ReasoningSummaries,
        Capability::UsageDetail,
        Capability::SamplingControls,
    ];
    profile_with_capabilities(dialect, &supported)
}

pub fn streaming_profile(dialect: WireDialect) -> ProviderProfile {
    profile_with_capabilities(dialect, &[Capability::Streaming])
}

fn profile_with_capabilities(dialect: WireDialect, supported: &[Capability]) -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new(match dialect {
            WireDialect::GeminiInteractionsV1 => [0x71; 16],
            _ => [0x72; 16],
        })
        .expect("profile ID"),
        1,
        ProviderName::new("google".to_owned()).expect("provider"),
        ModelName::new("gemini-3.7-flash".to_owned()).expect("model"),
        dialect,
        CapabilityMatrix::new(supported, &[]).expect("capabilities"),
        CapabilityProvenance::Profiled,
        ModelLimits::new(1_000_000, 65_536, 128, 32, 20 * 1024 * 1024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("Google profile")
}

pub fn request(profile: &ProviderProfile, streaming: bool) -> ModelRequest {
    let required = if streaming { &[Capability::Streaming][..] } else { &[][..] };
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(required, &[], profile.limits()).expect("requested features"),
    )
    .expect("negotiation");
    let text = BoundedText::new("hello".to_owned(), ProtocolLimits::PRODUCTION).expect("text");
    let message =
        Message::new(Role::User, vec![ContentBlock::Text(text)], ProtocolLimits::PRODUCTION)
            .expect("message");
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new("google-test-request".to_owned()).expect("request ID"),
        vec![message],
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        RequestOptions::new(
            StructuredOutput::Text,
            ReasoningPolicy::Disabled,
            GenerationConfig::new(128, Vec::new(), None, None, None).expect("generation"),
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        ProtocolLimits::PRODUCTION,
    )
    .expect("request")
}

pub fn config(dialect: WireDialect, attempts: u32) -> GoogleConfig {
    config_at("https://generativelanguage.googleapis.com", dialect, attempts)
}

pub fn config_at(endpoint: &str, dialect: WireDialect, attempts: u32) -> GoogleConfig {
    config_at_with_profile(endpoint, profile(dialect), attempts)
}

pub fn config_at_with_profile(
    endpoint: &str,
    profile: ProviderProfile,
    attempts: u32,
) -> GoogleConfig {
    GoogleConfig::new(
        Endpoint::new(endpoint.to_owned()).expect("endpoint"),
        CredentialReference::new("google-test-key".to_owned()).expect("credential reference"),
        profile,
        HttpLimits::PRODUCTION,
        FramingLimits::PRODUCTION,
        RetryPolicy::new(
            attempts,
            [
                Duration::from_millis(1),
                Duration::from_millis(300),
                Duration::from_millis(300),
                Duration::from_secs(1),
            ],
            1024 * 1024,
        )
        .expect("retry policy"),
    )
    .expect("config")
}

#[derive(Clone, Debug)]
pub struct CapturedRequest {
    pub method: HttpMethod,
    pub endpoint: String,
    pub headers: Vec<(String, bool, Option<Vec<u8>>)>,
    pub body: Vec<u8>,
}

#[derive(Default)]
pub struct TransportState {
    responses: Mutex<VecDeque<Result<HttpResponse, ProviderCoreError>>>,
    captured: Mutex<Vec<CapturedRequest>>,
}

impl TransportState {
    pub fn with_responses(responses: Vec<Result<HttpResponse, ProviderCoreError>>) -> Arc<Self> {
        Arc::new(Self { responses: Mutex::new(responses.into()), captured: Mutex::new(Vec::new()) })
    }

    pub fn captures(&self) -> Vec<CapturedRequest> {
        self.captured.lock().expect("capture lock").clone()
    }
}

pub struct TestTransport(pub Arc<TransportState>);

impl HttpTransport for TestTransport {
    fn send<'a>(
        &'a self,
        request: HttpRequest,
        _cancellation: &'a CancellationToken,
    ) -> BoxFuture<'a, Result<HttpResponse, ProviderCoreError>> {
        Box::pin(async move {
            let headers = request
                .headers()
                .iter()
                .map(|header| {
                    (
                        header.name().as_str().to_owned(),
                        header.value().is_sensitive(),
                        header.value().nonsensitive_bytes().map(<[u8]>::to_vec),
                    )
                })
                .collect();
            self.0.captured.lock().expect("capture lock").push(CapturedRequest {
                method: request.method(),
                endpoint: request.endpoint().as_str().to_owned(),
                headers,
                body: request.body().to_vec(),
            });
            self.0.responses.lock().expect("response lock").pop_front().expect("response script")
        })
    }
}

#[derive(Default)]
pub struct TestCredentials {
    resolutions: Arc<std::sync::atomic::AtomicUsize>,
}

impl TestCredentials {
    pub fn counter(&self) -> Arc<std::sync::atomic::AtomicUsize> {
        Arc::clone(&self.resolutions)
    }
}

impl CredentialSource for TestCredentials {
    fn resolve(&self, _reference: &CredentialReference) -> Result<Credential, ProviderCoreError> {
        self.resolutions.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Credential::new(b"google-secret-marker".to_vec())
    }
}

pub fn response(status: u16, headers: &[(&str, &str)], chunks: Vec<Vec<u8>>) -> HttpResponse {
    let headers = headers
        .iter()
        .map(|(name, value)| {
            Header::new(
                HeaderName::new((*name).to_owned()).expect("header name"),
                value.as_bytes().to_vec(),
            )
            .expect("header")
        })
        .collect();
    HttpResponse::new(
        StatusCode::new(status).expect("status"),
        HttpHeaders::new(headers, HttpLimits::PRODUCTION).expect("headers"),
        Box::new(MemoryByteStream::new(chunks, HttpLimits::PRODUCTION).expect("body")),
        HttpLimits::PRODUCTION,
    )
    .expect("response")
}
