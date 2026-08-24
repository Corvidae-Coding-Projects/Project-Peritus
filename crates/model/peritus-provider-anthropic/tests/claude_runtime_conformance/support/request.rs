//! Runtime profile and request construction for process qualification.

use peritus_conformance::{ProviderConformanceError, ProviderScenario};
use peritus_model_protocol::{
    BoundedText, CachePolicy, CancellationKind, Capability, CapabilityMatrix, CapabilityProvenance,
    ContentBlock, GenerationConfig, JsonBounds, JsonSchema, Message, ModelLimits, ModelName,
    ModelRequest, OutputLimitEnforcement, ParallelToolPolicy, PersistencePolicy, ProtocolLimits,
    ProviderName, ProviderProfile, ReasoningPolicy, RequestId, RequestOptions,
    RequestedCapabilities, ResumeKind, Role, SchemaDialect, StateMode, StructuredOutput,
    ToolChoice, ToolDefinition, ToolName, WireDialect, negotiate,
};
use peritus_types::ProviderProfileId;

pub fn profile(
    scenario: ProviderScenario,
    identity: u8,
) -> Result<ProviderProfile, ProviderConformanceError> {
    ProviderProfile::new(
        ProviderProfileId::new([identity; 16])
            .map_err(|_| ProviderConformanceError::Infrastructure)?,
        1,
        ProviderName::new("anthropic".to_owned())
            .map_err(|_| ProviderConformanceError::Infrastructure)?,
        ModelName::new(format!("claude-runtime-{}", super::slug(scenario)))
            .map_err(|_| ProviderConformanceError::Infrastructure)?,
        WireDialect::AnthropicClaudeRuntime,
        CapabilityMatrix::new(
            &[Capability::ToolCalls, Capability::ParallelToolCalls, Capability::UsageDetail],
            &[],
        )
        .map_err(|_| ProviderConformanceError::Infrastructure)?,
        CapabilityProvenance::Profiled,
        ModelLimits::new(200_000, 32_000, 32, 8, 1)
            .map_err(|_| ProviderConformanceError::Infrastructure)?,
        OutputLimitEnforcement::Advisory,
        StateMode::StatelessReplay,
        ResumeKind::Unsupported,
        CancellationKind::BestEffortLocalAbort,
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)
}

pub fn request(
    profile: &ProviderProfile,
    with_tools: bool,
    canary: Option<&str>,
) -> Result<ModelRequest, ProviderConformanceError> {
    let required = if with_tools || canary.is_some() {
        &[Capability::ToolCalls, Capability::ParallelToolCalls, Capability::UsageDetail][..]
    } else {
        &[Capability::UsageDetail][..]
    };
    let negotiated = negotiate(
        profile,
        RequestedCapabilities::new(required, &[], profile.limits())
            .map_err(|_| ProviderConformanceError::Infrastructure)?,
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let system = message(Role::System, canary.unwrap_or("host policy"))?;
    let user = message(Role::User, canary.unwrap_or("look up 42"))?;
    let tools = if with_tools || canary.is_some() { vec![tool(canary)?] } else { Vec::new() };
    let tool_choice = if tools.is_empty() { ToolChoice::None } else { ToolChoice::Auto };
    let parallel = if tools.is_empty() {
        ParallelToolPolicy::Disabled
    } else {
        ParallelToolPolicy::Allowed(2)
    };
    ModelRequest::new(
        profile,
        negotiated,
        RequestId::new(canary.unwrap_or("claude-runtime-conformance").to_owned())
            .map_err(|_| ProviderConformanceError::Infrastructure)?,
        vec![system, user],
        tools,
        tool_choice,
        parallel,
        RequestOptions::new(
            StructuredOutput::Text,
            ReasoningPolicy::Disabled,
            GenerationConfig::new(128, Vec::new(), None, None, None)
                .map_err(|_| ProviderConformanceError::Infrastructure)?,
            CachePolicy::Disabled,
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        ProtocolLimits::PRODUCTION,
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)
}

fn message(role: Role, value: &str) -> Result<Message, ProviderConformanceError> {
    Message::new(
        role,
        vec![ContentBlock::Text(
            BoundedText::new(value.to_owned(), ProtocolLimits::PRODUCTION)
                .map_err(|_| ProviderConformanceError::Infrastructure)?,
        )],
        ProtocolLimits::PRODUCTION,
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)
}

fn tool(canary: Option<&str>) -> Result<ToolDefinition, ProviderConformanceError> {
    let schema = JsonSchema::parse(
        r#"{"additionalProperties":false,"properties":{"id":{"type":"string"}},"required":["id"],"type":"object"}"#,
        SchemaDialect::Draft202012,
        JsonBounds::schema(ProtocolLimits::PRODUCTION),
    )
    .map_err(|_| ProviderConformanceError::Infrastructure)?;
    let description = canary
        .map(|value| BoundedText::new(value.to_owned(), ProtocolLimits::PRODUCTION))
        .transpose()
        .map_err(|_| ProviderConformanceError::Infrastructure)?;
    Ok(ToolDefinition::new(
        ToolName::new("lookup".to_owned()).map_err(|_| ProviderConformanceError::Infrastructure)?,
        description,
        schema,
        true,
    ))
}
