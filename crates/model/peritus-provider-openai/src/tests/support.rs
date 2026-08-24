use std::fs;
use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use peritus_model_protocol::{
    CacheKey, CachePolicy, CancellationKind, CanonicalJson, Capability, CapabilityMatrix,
    CapabilityProvenance, CompletedToolCall, ContentBlock, GenerationConfig, JsonBounds,
    JsonSchema, MediaInput, MediaKind, MediaReferenceKind, MediaType, Message, ModelLimits,
    ModelName, ModelRequest, OutputLimitEnforcement, OutputName, ParallelToolPolicy,
    PersistencePolicy, ProtocolLimits, ProviderName, ProviderProfile, ReasoningEffort,
    ReasoningPolicy, ReasoningReplay, RequestId, RequestOptions, RequestedCapabilities, ResumeKind,
    Role, SchemaDialect, StateMode, StructuredOutput, SummaryPolicy, ToolCallId, ToolChoice,
    ToolDefinition, ToolName, ToolResult, WireDialect, negotiate,
};
use peritus_provider_core::{Credential, CredentialReference, CredentialSource, ProviderCoreError};
use peritus_types::ProviderProfileId;

pub const SECRET: &str = "openai-secret-canary";

pub fn block_on<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime")
        .block_on(future)
}

pub fn fixture(name: &str) -> Vec<u8> {
    fs::read(Path::new("fixtures/v1").join(name)).expect("fixture bytes")
}

pub struct StaticCredential {
    resolutions: AtomicU64,
}

impl StaticCredential {
    pub const fn new() -> Self {
        Self { resolutions: AtomicU64::new(0) }
    }

    pub fn resolutions(&self) -> u64 {
        self.resolutions.load(Ordering::SeqCst)
    }
}

impl CredentialSource for StaticCredential {
    fn resolve(&self, _reference: &CredentialReference) -> Result<Credential, ProviderCoreError> {
        self.resolutions.fetch_add(1, Ordering::SeqCst);
        Credential::new(SECRET.as_bytes().to_vec())
    }
}

pub fn credential_reference() -> CredentialReference {
    CredentialReference::new("openai-test-key".to_owned()).expect("credential reference")
}

pub fn profile_full() -> ProviderProfile {
    let capabilities = [
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
        Capability::ResumableResponse,
        Capability::UsageDetail,
        Capability::RateLimitDetail,
        Capability::StoredState,
        Capability::SamplingControls,
        Capability::ConfirmedCancellation,
    ];
    profile(
        &capabilities,
        StateMode::BackgroundResumable,
        ResumeKind::ExactCursor,
        CancellationKind::Confirmed,
    )
}

pub fn profile_minimal() -> ProviderProfile {
    profile(
        &[Capability::Streaming, Capability::UsageDetail, Capability::RateLimitDetail],
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
}

pub fn profile_streaming_only() -> ProviderProfile {
    profile(
        &[Capability::Streaming],
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
}

fn profile(
    capabilities: &[Capability],
    state: StateMode,
    resume: ResumeKind,
    cancellation: CancellationKind,
) -> ProviderProfile {
    let limits = model_limits();
    ProviderProfile::new(
        ProviderProfileId::new([7; 16]).expect("profile id"),
        1,
        ProviderName::new("openai".to_owned()).expect("provider"),
        ModelName::new("gpt-test".to_owned()).expect("model"),
        WireDialect::OpenAiResponses,
        CapabilityMatrix::new(capabilities, &[]).expect("capabilities"),
        CapabilityProvenance::Profiled,
        limits,
        OutputLimitEnforcement::ProviderEnforced,
        state,
        resume,
        cancellation,
    )
    .expect("profile")
}

pub fn minimal_request(profile: &ProviderProfile) -> ModelRequest {
    request_with_capabilities(profile, &[Capability::Streaming])
}

pub fn request_with_id(profile: &ProviderProfile, request_id: &str) -> ModelRequest {
    request_with_id_and_capabilities(profile, request_id, &[Capability::Streaming])
}

pub fn redaction_request(profile: &ProviderProfile, canary: &str) -> ModelRequest {
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(
            &[Capability::Streaming, Capability::PromptCaching],
            &[],
            model_limits(),
        )
        .expect("requested"),
    )
    .expect("negotiated");
    let message = Message::new(
        Role::User,
        vec![ContentBlock::Text(text(canary))],
        ProtocolLimits::PRODUCTION,
    )
    .expect("message");
    let options = RequestOptions::new(
        StructuredOutput::Text,
        ReasoningPolicy::Disabled,
        GenerationConfig::new(64, Vec::new(), None, None, None).expect("generation"),
        CachePolicy::Explicit(CacheKey::new(canary.to_owned()).expect("cache key")),
        PersistencePolicy::LOCAL_FIRST,
        None,
        Vec::new(),
    );
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new(canary.to_owned()).expect("request id"),
        vec![message],
        Vec::new(),
        ToolChoice::Auto,
        ParallelToolPolicy::Disabled,
        options,
        ProtocolLimits::PRODUCTION,
    )
    .expect("redaction request")
}

#[allow(
    clippy::too_many_lines,
    reason = "the realistic golden fixture visibly binds every supported request family"
)]
pub fn realistic_request(profile: &ProviderProfile) -> ModelRequest {
    let selected = [
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
        Capability::StoredState,
        Capability::SamplingControls,
    ];
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(&selected, &[], model_limits()).expect("requested"),
    )
    .expect("negotiated");
    let call_id = ToolCallId::new("call-lookup".to_owned()).expect("call id");
    let messages = vec![
        message(Role::Developer, vec![ContentBlock::Text(text("Follow the contract."))]),
        message(
            Role::User,
            vec![
                ContentBlock::Text(text("Inspect this.")),
                ContentBlock::Image(
                    MediaInput::referenced(
                        MediaKind::Image,
                        MediaType::new("image/png".to_owned()).expect("media type"),
                        MediaReferenceKind::HttpsUrl,
                        "https://example.invalid/image.png".to_owned(),
                        None,
                    )
                    .expect("image"),
                ),
                ContentBlock::Audio(
                    MediaInput::inline(
                        MediaKind::Audio,
                        MediaType::new("audio/wav".to_owned()).expect("media type"),
                        vec![1, 2, 3],
                        ProtocolLimits::PRODUCTION,
                    )
                    .expect("audio"),
                ),
                ContentBlock::Document(
                    MediaInput::referenced(
                        MediaKind::Document,
                        MediaType::new("application/pdf".to_owned()).expect("media type"),
                        MediaReferenceKind::ProviderFile,
                        "file-document".to_owned(),
                        None,
                    )
                    .expect("document"),
                ),
            ],
        ),
        message(
            Role::Assistant,
            vec![
                ContentBlock::Reasoning(
                    ReasoningReplay::new(
                        Some(text("Prior reasoning summary.")),
                        b"encrypted-reasoning-state".to_vec(),
                        ProtocolLimits::PRODUCTION,
                    )
                    .expect("reasoning replay"),
                ),
                ContentBlock::ToolCall(
                    CompletedToolCall::new(
                        call_id.clone(),
                        ToolName::new("lookup".to_owned()).expect("tool name"),
                        json(r#"{"id":"prior"}"#),
                    )
                    .expect("tool call"),
                ),
            ],
        ),
        message(
            Role::Tool,
            vec![ContentBlock::ToolResult(ToolResult::new(
                call_id,
                json(r#"{"name":"record"}"#),
                false,
            ))],
        ),
    ];
    let tool_name = ToolName::new("lookup".to_owned()).expect("tool name");
    let tools = vec![ToolDefinition::new(
        tool_name.clone(),
        Some(text("Look up a record")),
        schema(
            r#"{"additionalProperties":false,"properties":{"id":{"type":"string"}},"required":["id"],"type":"object"}"#,
        ),
        true,
    )];
    let options = RequestOptions::new(
        StructuredOutput::JsonSchema {
            name: OutputName::new("answer".to_owned()).expect("output name"),
            schema: schema(
                r#"{"additionalProperties":false,"properties":{"answer":{"type":"string"}},"required":["answer"],"type":"object"}"#,
            ),
            strict: true,
        },
        ReasoningPolicy::Effort { effort: ReasoningEffort::High, summary: SummaryPolicy::Concise },
        GenerationConfig::new(256, Vec::new(), None, Some(200_000), Some(900_000))
            .expect("generation"),
        CachePolicy::Ephemeral { ttl_seconds: 1_800 },
        PersistencePolicy::new(true, true).expect("persistence"),
        None,
        Vec::new(),
    );
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new("request-realistic".to_owned()).expect("request id"),
        messages,
        tools,
        ToolChoice::Specific(tool_name),
        ParallelToolPolicy::Allowed(8),
        options,
        ProtocolLimits::PRODUCTION,
    )
    .expect("realistic request")
}

pub fn request_with_capabilities(
    profile: &ProviderProfile,
    capabilities: &[Capability],
) -> ModelRequest {
    request_with_id_and_capabilities(profile, "request-test", capabilities)
}

fn request_with_id_and_capabilities(
    profile: &ProviderProfile,
    request_id: &str,
    capabilities: &[Capability],
) -> ModelRequest {
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(capabilities, &[], model_limits()).expect("requested"),
    )
    .expect("negotiated");
    let text =
        peritus_model_protocol::BoundedText::new("hello".to_owned(), ProtocolLimits::PRODUCTION)
            .expect("text");
    let message =
        Message::new(Role::User, vec![ContentBlock::Text(text)], ProtocolLimits::PRODUCTION)
            .expect("message");
    let options = RequestOptions::new(
        StructuredOutput::Text,
        ReasoningPolicy::Disabled,
        GenerationConfig::new(64, Vec::new(), None, None, None).expect("generation"),
        CachePolicy::Disabled,
        PersistencePolicy::LOCAL_FIRST,
        None,
        Vec::new(),
    );
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new(request_id.to_owned()).expect("request id"),
        vec![message],
        Vec::new(),
        ToolChoice::Auto,
        ParallelToolPolicy::Disabled,
        options,
        ProtocolLimits::PRODUCTION,
    )
    .expect("request")
}

pub fn model_limits() -> ModelLimits {
    ModelLimits::new(128_000, 4_096, 32, 8, 16 * 1024 * 1024).expect("model limits")
}

fn text(value: &str) -> peritus_model_protocol::BoundedText {
    peritus_model_protocol::BoundedText::new(value.to_owned(), ProtocolLimits::PRODUCTION)
        .expect("bounded text")
}

fn message(role: Role, content: Vec<ContentBlock>) -> Message {
    Message::new(role, content, ProtocolLimits::PRODUCTION).expect("message")
}

fn json(value: &str) -> CanonicalJson {
    CanonicalJson::parse(value, JsonBounds::value(ProtocolLimits::PRODUCTION))
        .expect("canonical JSON")
}

fn schema(value: &str) -> JsonSchema {
    JsonSchema::parse(
        value,
        SchemaDialect::Draft202012,
        JsonBounds::schema(ProtocolLimits::PRODUCTION),
    )
    .expect("JSON schema")
}
