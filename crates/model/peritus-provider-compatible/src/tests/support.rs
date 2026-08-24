use std::fs;
use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use peritus_model_protocol::{
    CachePolicy, CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance,
    ContentBlock, GenerationConfig, JsonBounds, JsonSchema, Message, ModelLimits, ModelName,
    ModelRequest, OutputLimitEnforcement, ParallelToolPolicy, PersistencePolicy, ProtocolLimits,
    ProviderName, ProviderProfile, ReasoningPolicy, RequestId, RequestOptions,
    RequestedCapabilities, ResumeKind, Role, SchemaDialect, StateMode, StructuredOutput,
    ToolChoice, ToolDefinition, ToolName, WireDialect, negotiate,
};
use peritus_provider_core::{Credential, CredentialReference, CredentialSource, ProviderCoreError};
use peritus_types::ProviderProfileId;

pub const SECRET: &str = "compatible-secret-canary";

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
    CredentialReference::new("compatible-test-key".to_owned()).expect("credential reference")
}

pub fn responses_profile(capabilities: &[Capability]) -> ProviderProfile {
    profile(WireDialect::CompatibleResponses, capabilities, [7; 16])
}

pub fn chat_profile(capabilities: &[Capability]) -> ProviderProfile {
    profile(WireDialect::CompatibleChatCompletions, capabilities, [8; 16])
}

fn profile(
    dialect: WireDialect,
    capabilities: &[Capability],
    identity: [u8; 16],
) -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new(identity).expect("profile id"),
        7,
        ProviderName::new("compatible-test".to_owned()).expect("provider"),
        ModelName::new("model-test".to_owned()).expect("model"),
        dialect,
        CapabilityMatrix::new(capabilities, &[]).expect("capabilities"),
        CapabilityProvenance::Profiled,
        model_limits(),
        OutputLimitEnforcement::ProviderEnforced,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("profile")
}

pub fn minimal_request(profile: &ProviderProfile) -> ModelRequest {
    if profile.capabilities().supports(Capability::UsageDetail) {
        request_with_capabilities(profile, &[Capability::Streaming, Capability::UsageDetail])
    } else {
        request_with_capabilities(profile, &[Capability::Streaming])
    }
}

pub fn request_with_id(profile: &ProviderProfile, identity: &str) -> ModelRequest {
    request_with(profile, identity, &[Capability::Streaming], "hello", Vec::new())
}

pub fn redaction_request(profile: &ProviderProfile, canary: &str) -> ModelRequest {
    let schema = JsonSchema::parse(
        r#"{"additionalProperties":false,"properties":{"id":{"type":"string"}},"required":["id"],"type":"object"}"#,
        SchemaDialect::Draft202012,
        JsonBounds::schema(ProtocolLimits::PRODUCTION),
    )
    .expect("schema");
    let description =
        peritus_model_protocol::BoundedText::new(canary.to_owned(), ProtocolLimits::PRODUCTION)
            .expect("description");
    let tool = ToolDefinition::new(
        ToolName::new("lookup".to_owned()).expect("tool name"),
        Some(description),
        schema,
        true,
    );
    request_with(
        profile,
        canary,
        &[Capability::Streaming, Capability::ToolCalls],
        canary,
        vec![tool],
    )
}

pub fn tool_request(profile: &ProviderProfile) -> ModelRequest {
    let schema = JsonSchema::parse(
        r#"{"additionalProperties":false,"properties":{"id":{"type":"string"}},"required":["id"],"type":"object"}"#,
        SchemaDialect::Draft202012,
        JsonBounds::schema(ProtocolLimits::PRODUCTION),
    )
    .expect("schema");
    let tool = ToolDefinition::new(
        ToolName::new("lookup".to_owned()).expect("tool name"),
        None,
        schema,
        true,
    );
    request_with(
        profile,
        "request-tool",
        &[Capability::Streaming, Capability::ToolCalls, Capability::UsageDetail],
        "look up 42",
        vec![tool],
    )
}

pub fn request_with_capabilities(
    profile: &ProviderProfile,
    capabilities: &[Capability],
) -> ModelRequest {
    request_with(profile, "request-test", capabilities, "hello", Vec::new())
}

fn request_with(
    profile: &ProviderProfile,
    identity: &str,
    capabilities: &[Capability],
    prompt: &str,
    tools: Vec<ToolDefinition>,
) -> ModelRequest {
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(capabilities, &[], model_limits()).expect("requested"),
    )
    .expect("negotiated");
    let text =
        peritus_model_protocol::BoundedText::new(prompt.to_owned(), ProtocolLimits::PRODUCTION)
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
        RequestId::new(identity.to_owned()).expect("request id"),
        vec![message],
        tools,
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
