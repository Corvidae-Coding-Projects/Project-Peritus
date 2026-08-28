//! Runtime-only profile and request builders.

use peritus_model_protocol::{
    CachePolicy, CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance,
    ContentBlock, GenerationConfig, JsonBounds, JsonSchema, MediaInput, MediaKind, MediaType,
    Message, ModelRequest, OutputLimitEnforcement, ParallelToolPolicy, PersistencePolicy,
    ProtocolLimits, ProviderName, ProviderProfile, ReasoningPolicy, RequestId, RequestOptions,
    RequestedCapabilities, ResumeKind, Role, StateMode, StructuredOutput, ToolChoice,
    ToolDefinition, ToolName, WireDialect, negotiate,
};
use peritus_types::ProviderProfileId;

use super::support::model_limits;
pub fn codex_profile(model: &str, tools: bool) -> ProviderProfile {
    let supported = if tools {
        vec![Capability::ToolCalls, Capability::ParallelToolCalls, Capability::UsageDetail]
    } else {
        vec![Capability::UsageDetail]
    };
    ProviderProfile::new(
        ProviderProfileId::new([11; 16]).expect("profile id"),
        1,
        ProviderName::new("openai".to_owned()).expect("provider"),
        peritus_model_protocol::ModelName::new(model.to_owned()).expect("model"),
        WireDialect::OpenAiCodexRuntime,
        CapabilityMatrix::new(&supported, &[]).expect("capabilities"),
        CapabilityProvenance::Profiled,
        model_limits(),
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("Codex runtime profile")
}

pub fn codex_image_profile(model: &str) -> ProviderProfile {
    ProviderProfile::new(
        ProviderProfileId::new([12; 16]).expect("profile id"),
        1,
        ProviderName::new("openai".to_owned()).expect("provider"),
        peritus_model_protocol::ModelName::new(model.to_owned()).expect("model"),
        WireDialect::OpenAiCodexRuntime,
        CapabilityMatrix::new(&[Capability::ImageInput, Capability::UsageDetail], &[])
            .expect("capabilities"),
        CapabilityProvenance::Profiled,
        model_limits(),
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .expect("Codex image profile")
}

pub fn codex_tool_request(profile: &ProviderProfile, request_id: &str) -> ModelRequest {
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(
            &[Capability::ToolCalls, Capability::ParallelToolCalls, Capability::UsageDetail],
            &[],
            model_limits(),
        )
        .expect("requested"),
    )
    .expect("negotiated");
    let tool = ToolDefinition::new(
        ToolName::new("lookup".to_owned()).expect("tool name"),
        Some(text("Look up a value")),
        schema(
            r#"{"additionalProperties":false,"properties":{"value":{"type":"string"}},"required":["value"],"type":"object"}"#,
        ),
        true,
    );
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new(request_id.to_owned()).expect("request id"),
        vec![message(Role::User, vec![ContentBlock::Text(text("Use the host tool"))])],
        vec![tool],
        ToolChoice::Auto,
        ParallelToolPolicy::Allowed(2),
        runtime_options(),
        ProtocolLimits::PRODUCTION,
    )
    .expect("Codex tool request")
}

pub fn codex_image_request(profile: &ProviderProfile, request_id: &str) -> ModelRequest {
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(&[Capability::ImageInput], &[], model_limits())
            .expect("requested"),
    )
    .expect("negotiated");
    let media = MediaInput::inline(
        MediaKind::Image,
        MediaType::new("image/png".to_owned()).expect("media type"),
        b"bounded-image-bytes".to_vec(),
        ProtocolLimits::PRODUCTION,
    )
    .expect("inline image");
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new(request_id.to_owned()).expect("request id"),
        vec![message(
            Role::User,
            vec![ContentBlock::Text(text("Describe attachment zero")), ContentBlock::Image(media)],
        )],
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        runtime_options(),
        ProtocolLimits::PRODUCTION,
    )
    .expect("Codex image request")
}

fn runtime_options() -> RequestOptions {
    RequestOptions::new(
        StructuredOutput::Text,
        ReasoningPolicy::Disabled,
        GenerationConfig::new(64, Vec::new(), None, None, None).expect("generation"),
        CachePolicy::Disabled,
        PersistencePolicy::LOCAL_FIRST,
        None,
        Vec::new(),
    )
}

fn text(value: &str) -> peritus_model_protocol::BoundedText {
    peritus_model_protocol::BoundedText::new(value.to_owned(), ProtocolLimits::PRODUCTION)
        .expect("text")
}

fn message(role: Role, content: Vec<ContentBlock>) -> Message {
    Message::new(role, content, ProtocolLimits::PRODUCTION).expect("message")
}

fn schema(value: &str) -> JsonSchema {
    JsonSchema::parse(
        value,
        peritus_model_protocol::SchemaDialect::Draft202012,
        JsonBounds::schema(ProtocolLimits::PRODUCTION),
    )
    .expect("canonical schema")
}
