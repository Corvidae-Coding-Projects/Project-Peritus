//! Shared canonical-request fixture constructors.

use peritus_model_protocol::{
    BoundedText, CachePolicy, CancellationKind, CanonicalJson, Capability, CapabilityMatrix,
    CapabilityProvenance, CompletedToolCall, ContentBlock, EventId, ExtensionName,
    GenerationConfig, JsonBounds, JsonSchema, MediaInput, MediaKind, MediaReferenceKind, MediaType,
    Message, ModelLimits, ModelName, ModelRequest, OutputLimitEnforcement, OutputName,
    ParallelToolPolicy, PersistencePolicy, ProtocolLimits, ProviderExtension, ProviderName,
    ProviderProfile, ReasoningEffort, ReasoningPolicy, ReasoningReplay, RequestId, RequestOptions,
    RequestedCapabilities, ResponseId, ResumeKind, Role, SchemaDialect, StateMode,
    StructuredOutput, SummaryPolicy, ToolCallId, ToolChoice, ToolDefinition, ToolName, ToolResult,
    WireDialect, negotiate,
};
use peritus_types::{ArtifactId, ProviderProfileId, Sha256Digest};

pub fn all_capabilities() -> Vec<Capability> {
    vec![
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
        Capability::ConfirmedCancellation,
        Capability::UsageDetail,
        Capability::RateLimitDetail,
        Capability::StoredState,
        Capability::ProviderExtensions,
        Capability::SamplingControls,
    ]
}

pub fn profile() -> ProviderProfile {
    profile_with(ProviderProfileId::new([0x11; 16]).expect("profile ID"), 7)
}

pub fn profile_with(profile_id: ProviderProfileId, revision: u64) -> ProviderProfile {
    profile_for(
        profile_id,
        revision,
        WireDialect::CompatibleResponses,
        OutputLimitEnforcement::ProviderEnforced,
    )
}

pub fn runtime_profile(dialect: WireDialect) -> ProviderProfile {
    profile_for(
        ProviderProfileId::new([0x11; 16]).expect("profile ID"),
        7,
        dialect,
        OutputLimitEnforcement::Advisory,
    )
}

fn profile_for(
    profile_id: ProviderProfileId,
    revision: u64,
    dialect: WireDialect,
    output_limit: OutputLimitEnforcement,
) -> ProviderProfile {
    ProviderProfile::new(
        profile_id,
        revision,
        ProviderName::new("fixture-provider".to_owned()).expect("provider"),
        ModelName::new("fixture-model-v1".to_owned()).expect("model"),
        dialect,
        CapabilityMatrix::new(&all_capabilities(), &[]).expect("capabilities"),
        CapabilityProvenance::Profiled,
        ModelLimits::new(100_000, 16_384, 64, 8, 4 * 1024 * 1024).expect("profile limits"),
        output_limit,
        StateMode::BackgroundResumable,
        ResumeKind::ExactCursor,
        CancellationKind::Confirmed,
    )
    .expect("profile")
}

pub fn request_id() -> RequestId {
    RequestId::new("fixture-request-observability-only".to_owned()).expect("request ID")
}

pub fn minimal_request(profile: &ProviderProfile) -> ModelRequest {
    let limits = ProtocolLimits::PRODUCTION;
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(&[], &[], requested_limits()).expect("request capabilities"),
    )
    .expect("negotiation");
    ModelRequest::new(
        profile,
        negotiated,
        request_id(),
        vec![
            Message::new(Role::User, vec![ContentBlock::Text(text("hello"))], limits)
                .expect("message"),
        ],
        Vec::new(),
        ToolChoice::None,
        ParallelToolPolicy::Disabled,
        RequestOptions::new(
            StructuredOutput::Text,
            ReasoningPolicy::Disabled,
            GenerationConfig::new(1, Vec::new(), None, None, None).expect("generation"),
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        limits,
    )
    .expect("minimal request")
}

pub fn boundary_request(profile: &ProviderProfile) -> ModelRequest {
    let limits = ProtocolLimits::PRODUCTION;
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(&[], &[], requested_limits()).expect("request capabilities"),
    )
    .expect("negotiation");
    let stops = (0..8).map(|_| text("x")).collect();
    ModelRequest::new(
        profile,
        negotiated,
        request_id(),
        vec![
            Message::new(Role::User, vec![ContentBlock::Text(text("boundary"))], limits)
                .expect("message"),
        ],
        Vec::new(),
        ToolChoice::Auto,
        ParallelToolPolicy::Disabled,
        RequestOptions::new(
            StructuredOutput::JsonObject,
            ReasoningPolicy::Disabled,
            GenerationConfig::new(4_096, stops, None, None, None).expect("generation"),
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        limits,
    )
    .expect("boundary request")
}

#[allow(clippy::too_many_lines, reason = "one complete compatibility request fixture")]
pub fn realistic_request(profile: &ProviderProfile) -> ModelRequest {
    let limits = ProtocolLimits::PRODUCTION;
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(&all_capabilities(), &[], requested_limits())
            .expect("request capabilities"),
    )
    .expect("negotiation");
    let schema = JsonSchema::parse(
        r#"{"additionalProperties":false,"properties":{"path":{"type":"string"}},"required":["path"],"type":"object"}"#,
        SchemaDialect::Draft202012,
        JsonBounds::schema(limits),
    )
    .expect("schema");
    let tool_name = ToolName::new("read_file".to_owned()).expect("tool name");
    let tool =
        ToolDefinition::new(tool_name.clone(), Some(text("Read one file")), schema.clone(), true);
    let call_id = ToolCallId::new("call-1".to_owned()).expect("call ID");
    let call = CompletedToolCall::new(
        call_id.clone(),
        tool_name.clone(),
        json(r#"{"path":"README.md"}"#, limits),
    )
    .expect("tool call");
    let image = MediaInput::inline(
        MediaKind::Image,
        MediaType::new("image/png".to_owned()).expect("media type"),
        vec![1, 2, 3, 4],
        limits,
    )
    .expect("image");
    let audio = MediaInput::referenced(
        MediaKind::Audio,
        MediaType::new("audio/wav".to_owned()).expect("media type"),
        MediaReferenceKind::HttpsUrl,
        "https://example.invalid/audio.wav".to_owned(),
        Some(Sha256Digest::new([0x22; 32])),
    )
    .expect("audio");
    let document = MediaInput::artifact(
        MediaKind::Document,
        MediaType::new("application/pdf".to_owned()).expect("media type"),
        ArtifactId::new([0x33; 16]).expect("artifact ID"),
        Sha256Digest::new([0x44; 32]),
    );
    let reasoning = ReasoningReplay::new(Some(text("reviewed")), vec![9, 8, 7], limits)
        .expect("reasoning replay");
    let messages = vec![
        Message::new(Role::System, vec![ContentBlock::Text(text("Be exact"))], limits)
            .expect("system message"),
        Message::new(
            Role::User,
            vec![
                ContentBlock::Text(text("Inspect these inputs")),
                ContentBlock::Image(image),
                ContentBlock::Audio(audio),
                ContentBlock::Document(document),
            ],
            limits,
        )
        .expect("user message"),
        Message::new(
            Role::Assistant,
            vec![ContentBlock::Reasoning(reasoning), ContentBlock::ToolCall(call)],
            limits,
        )
        .expect("assistant message"),
        Message::new(
            Role::Tool,
            vec![ContentBlock::ToolResult(ToolResult::new(
                call_id,
                json(r#"{"content":"fixture"}"#, limits),
                false,
            ))],
            limits,
        )
        .expect("tool message"),
    ];
    let extension = ProviderExtension::new(
        ExtensionName::new("fixture.mode".to_owned()).expect("extension name"),
        json(r#"{"enabled":true}"#, limits),
    );
    let options = RequestOptions::new(
        StructuredOutput::JsonSchema {
            name: OutputName::new("answer".to_owned()).expect("output name"),
            schema,
            strict: true,
        },
        ReasoningPolicy::Effort { effort: ReasoningEffort::High, summary: SummaryPolicy::Concise },
        GenerationConfig::new(4_096, vec![text("<END>")], Some(42), Some(250_000), Some(900_000))
            .expect("generation"),
        CachePolicy::Automatic,
        PersistencePolicy::new(true, true).expect("persistence"),
        Some(
            peritus_model_protocol::Continuation::new(
                ResponseId::new("response-1".to_owned()).expect("response ID"),
                Some(EventId::new("event-11".to_owned()).expect("event ID")),
                Some(11),
            )
            .expect("continuation"),
        ),
        vec![extension],
    );
    ModelRequest::new(
        profile,
        negotiated,
        request_id(),
        messages,
        vec![tool],
        ToolChoice::Specific(tool_name),
        ParallelToolPolicy::Allowed(4),
        options,
        limits,
    )
    .expect("realistic request")
}

fn requested_limits() -> ModelLimits {
    ModelLimits::new(80_000, 4_096, 32, 4, 2 * 1024 * 1024).expect("requested limits")
}

fn text(value: &str) -> BoundedText {
    BoundedText::new(value.to_owned(), ProtocolLimits::PRODUCTION).expect("text")
}

fn json(value: &str, limits: ProtocolLimits) -> CanonicalJson {
    CanonicalJson::parse(value, JsonBounds::value(limits)).expect("JSON")
}
