//! Adapter-owned deterministic fixtures and transports for unit tests.

use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use peritus_model_protocol::{
    BoundedText, CachePolicy, CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance,
    ContentBlock, GenerationConfig, JsonBounds, JsonSchema, Message, ModelLimits, ModelName,
    ModelRequest, OutputLimitEnforcement, ParallelToolPolicy, PersistencePolicy, ProtocolLimits,
    ProviderName, ProviderProfile, ReasoningEffort, ReasoningPolicy, RequestId, RequestOptions,
    RequestedCapabilities, ResumeKind, Role, SchemaDialect, StateMode, StructuredOutput,
    SummaryPolicy, ToolChoice, ToolDefinition, ToolName, WireDialect, negotiate,
};
use peritus_provider_core::{
    BoxFuture, CancellationToken, Credential, CredentialReference, CredentialSource, Endpoint,
    FramingLimits, Header, HeaderName, HttpHeaders, HttpLimits, HttpMethod, HttpRequest,
    HttpResponse, HttpTransport, MemoryByteStream, ProviderCoreError, RetryPolicy, StatusCode,
};
use peritus_types::ProviderProfileId;

use crate::{AnthropicBeta, AnthropicConfig};

pub fn block_on<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

pub fn fixture(name: &str) -> Vec<u8> {
    let package_relative = std::path::Path::new("fixtures/v1").join(name);
    let workspace_relative =
        std::path::Path::new("crates/model/peritus-provider-anthropic/fixtures/v1").join(name);
    std::fs::read(&package_relative)
        .or_else(|_| std::fs::read(&workspace_relative))
        .expect("versioned Anthropic fixture")
}

pub fn profile() -> ProviderProfile {
    let supported = [
        Capability::Streaming,
        Capability::ToolCalls,
        Capability::ParallelToolCalls,
        Capability::StrictStructuredOutput,
        Capability::PromptCaching,
        Capability::ImageInput,
        Capability::DocumentInput,
        Capability::ReasoningControls,
        Capability::UsageDetail,
        Capability::RateLimitDetail,
        Capability::SamplingControls,
    ];
    ProviderProfile::new(
        ProviderProfileId::new([0xA5; 16]).expect("profile ID"),
        1,
        ProviderName::new("anthropic".to_owned()).expect("provider"),
        ModelName::new("claude-sonnet-4-5".to_owned()).expect("model"),
        WireDialect::AnthropicMessages,
        CapabilityMatrix::new(&supported, &[]).expect("capabilities"),
        CapabilityProvenance::Profiled,
        ModelLimits::new(200_000, 32_000, 32, 8, 8 * 1024 * 1024).expect("limits"),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("Anthropic profile")
}

pub fn runtime_profile() -> ProviderProfile {
    let supported = [
        Capability::ToolCalls,
        Capability::ParallelToolCalls,
        Capability::PromptCaching,
        Capability::ReasoningControls,
        Capability::UsageDetail,
    ];
    ProviderProfile::new(
        ProviderProfileId::new([0xC1; 16]).expect("profile ID"),
        1,
        ProviderName::new("anthropic".to_owned()).expect("provider"),
        ModelName::new("claude-runtime-test".to_owned()).expect("model"),
        WireDialect::AnthropicClaudeRuntime,
        CapabilityMatrix::new(&supported, &[]).expect("capabilities"),
        CapabilityProvenance::Profiled,
        ModelLimits::new(200_000, 32_000, 32, 8, 1).expect("limits"),
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("Claude runtime profile")
}

pub fn runtime_request(profile: &ProviderProfile, with_tools: bool) -> ModelRequest {
    let mut required =
        vec![Capability::PromptCaching, Capability::ReasoningControls, Capability::UsageDetail];
    if with_tools {
        required.extend([Capability::ToolCalls, Capability::ParallelToolCalls]);
    }
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(&required, &[], profile.limits()).expect("requested features"),
    )
    .expect("negotiation");
    let system = Message::new(
        Role::System,
        vec![ContentBlock::Text(
            BoundedText::new("host policy".to_owned(), ProtocolLimits::PRODUCTION)
                .expect("system text"),
        )],
        ProtocolLimits::PRODUCTION,
    )
    .expect("system message");
    let user = Message::new(
        Role::User,
        vec![ContentBlock::Text(
            BoundedText::new("look up 42".to_owned(), ProtocolLimits::PRODUCTION)
                .expect("user text"),
        )],
        ProtocolLimits::PRODUCTION,
    )
    .expect("user message");
    let tools = if with_tools {
        let schema = JsonSchema::parse(
            r#"{"additionalProperties":false,"properties":{"id":{"type":"string"}},"required":["id"],"type":"object"}"#,
            SchemaDialect::Draft202012,
            JsonBounds::schema(ProtocolLimits::PRODUCTION),
        )
        .expect("tool schema");
        vec![ToolDefinition::new(
            ToolName::new("lookup".to_owned()).expect("tool name"),
            None,
            schema,
            true,
        )]
    } else {
        Vec::new()
    };
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new("claude-runtime-request".to_owned()).expect("request ID"),
        vec![system, user],
        tools,
        if with_tools { ToolChoice::Auto } else { ToolChoice::None },
        if with_tools { ParallelToolPolicy::Allowed(2) } else { ParallelToolPolicy::Disabled },
        RequestOptions::new(
            StructuredOutput::Text,
            ReasoningPolicy::Effort { effort: ReasoningEffort::High, summary: SummaryPolicy::None },
            GenerationConfig::new(128, Vec::new(), None, None, None).expect("generation"),
            CachePolicy::Automatic,
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        ProtocolLimits::PRODUCTION,
    )
    .expect("runtime request")
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
        RequestId::new("anthropic-test-request".to_owned()).expect("request ID"),
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

pub fn config(attempts: u32, betas: Vec<AnthropicBeta>) -> AnthropicConfig {
    config_at("https://api.anthropic.com", attempts, betas)
}

pub fn config_at(endpoint: &str, attempts: u32, betas: Vec<AnthropicBeta>) -> AnthropicConfig {
    AnthropicConfig::new(
        Endpoint::new(endpoint.to_owned()).expect("endpoint"),
        CredentialReference::new("anthropic-test-key".to_owned()).expect("credential reference"),
        profile(),
        betas,
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
            self.0.responses.lock().expect("response lock").pop_front().expect("scripted response")
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
        Credential::new(b"sk-ant-secret-marker".to_vec())
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
