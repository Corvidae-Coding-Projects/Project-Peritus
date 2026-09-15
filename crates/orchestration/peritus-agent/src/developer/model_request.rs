//! Provider request construction for developer and semantic-checkpoint turns.

use peritus_model_protocol::{
    CachePolicy, Capability, GenerationConfig, Message, ModelRequest, ParallelToolPolicy,
    PersistencePolicy, ProtocolLimits, ReasoningEffort, ReasoningPolicy, RequestId, RequestOptions,
    StructuredOutput, SummaryPolicy, ToolChoice,
};

use super::{DeveloperLoopError, DeveloperLoopRequest};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ModelTurnKind {
    Developer,
    SemanticCompaction,
}

#[allow(
    clippy::too_many_arguments,
    reason = "one immutable provider request keeps its negotiated inputs explicit"
)]
pub(super) fn build_model_request(
    request: &DeveloperLoopRequest,
    messages: &[Message],
    profile: &peritus_model_protocol::ProviderProfile,
    negotiated: peritus_model_protocol::NegotiatedCapabilities,
    protocol_limits: ProtocolLimits,
    turn: u16,
    attempt: u8,
    kind: ModelTurnKind,
    required_tool: Option<&str>,
    selected_effort: Option<ReasoningEffort>,
) -> Result<ModelRequest, DeveloperLoopError> {
    let mut messages = messages.to_vec();
    if kind == ModelTurnKind::Developer
        && attempt > 1
        && let Some(required) = required_tool
    {
        messages.insert(
            1,
            Message::new(
                peritus_model_protocol::Role::System,
                vec![peritus_model_protocol::ContentBlock::Text(
                    peritus_model_protocol::BoundedText::new(
                        format!(
                            "CURRENT PROVIDER RETRY\nattempt={attempt}; required_tool={required}\nThis is a fresh provider retry of the same host step. Return exactly one call to the declared `{required}` tool now and no terminal response. The host will execute the call and return its result on the next turn."
                        ),
                        protocol_limits,
                    )?,
                )],
                protocol_limits,
            )?,
        );
    }
    let parallel_tools = if kind == ModelTurnKind::Developer
        && required_tool.is_none()
        && negotiated.includes(Capability::ParallelToolCalls)
    {
        ParallelToolPolicy::Allowed(negotiated.limits().max_parallel_tool_calls())
    } else {
        ParallelToolPolicy::Disabled
    };
    let reasoning = if negotiated.includes(Capability::ReasoningControls) {
        ReasoningPolicy::Effort {
            effort: selected_effort.unwrap_or(ReasoningEffort::High),
            summary: SummaryPolicy::None,
        }
    } else {
        if selected_effort.is_some() {
            return Err(DeveloperLoopError::Context(
                "selected reasoning effort is not supported by negotiated capabilities".to_owned(),
            ));
        }
        ReasoningPolicy::Disabled
    };
    let developer_tool_choice = match required_tool {
        Some(required) => request
            .tools
            .iter()
            .find(|tool| tool.name().as_str() == required)
            .map(|tool| ToolChoice::Specific(tool.name().clone()))
            .ok_or_else(|| {
                DeveloperLoopError::Context(format!(
                    "executor required undeclared developer tool: {required}"
                ))
            })?,
        None => ToolChoice::Auto,
    };
    let (request_id, tools, tool_choice, output_tokens) = match kind {
        ModelTurnKind::Developer => (
            format!("{}-{turn}-attempt-{attempt}", request.request_prefix),
            request.tools.clone(),
            developer_tool_choice,
            request.limits.max_output_tokens(),
        ),
        ModelTurnKind::SemanticCompaction => (
            format!("{}-semantic-compaction-{turn}-attempt-{attempt}", request.request_prefix),
            Vec::new(),
            ToolChoice::None,
            request.limits.max_output_tokens().min(8_192),
        ),
    };
    Ok(ModelRequest::new(
        profile,
        negotiated,
        RequestId::new(request_id)?,
        messages,
        tools,
        tool_choice,
        parallel_tools,
        RequestOptions::new(
            StructuredOutput::Text,
            reasoning,
            GenerationConfig::new(
                profile.limits().max_output_tokens().min(output_tokens),
                Vec::new(),
                None,
                None,
                None,
            )?,
            if negotiated.includes(Capability::PromptCaching) {
                CachePolicy::Automatic
            } else {
                CachePolicy::Disabled
            },
            PersistencePolicy::LOCAL_FIRST,
            None,
            Vec::new(),
        ),
        protocol_limits,
    )?)
}
