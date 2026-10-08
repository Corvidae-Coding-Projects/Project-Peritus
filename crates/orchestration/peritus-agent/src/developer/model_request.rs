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
    attempt: u64,
    kind: ModelTurnKind,
    required_tool: Option<&str>,
    selected_effort: Option<ReasoningEffort>,
    remaining_tool_calls: u32,
) -> Result<ModelRequest, DeveloperLoopError> {
    let segment_prefix = request.limits.segment_request_prefix(&request.request_prefix);
    let parallel_tool_calls = negotiated
        .limits()
        .max_parallel_tool_calls()
        .min(remaining_tool_calls);
    let parallel_tools = if kind == ModelTurnKind::Developer
        && required_tool.is_none()
        && negotiated.includes(Capability::ParallelToolCalls)
        && parallel_tool_calls > 0
    {
        ParallelToolPolicy::Allowed(parallel_tool_calls)
    } else {
        ParallelToolPolicy::Disabled
    };
    let reasoning = if negotiated.includes(Capability::ReasoningControls) {
        ReasoningPolicy::Effort {
            effort: selected_effort.unwrap_or(ReasoningEffort::High),
            summary: if negotiated.includes(Capability::ReasoningSummaries) {
                SummaryPolicy::Auto
            } else {
                SummaryPolicy::None
            },
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
            format!("{segment_prefix}-{turn}-attempt-{attempt}"),
            request.tools.clone(),
            developer_tool_choice,
            request.limits.max_output_tokens(),
        ),
        ModelTurnKind::SemanticCompaction => (
            format!("{segment_prefix}-semantic-compaction-{turn}-attempt-{attempt}"),
            Vec::new(),
            ToolChoice::None,
            request.limits.max_output_tokens(),
        ),
    };
    let model_request = ModelRequest::new(
        profile,
        negotiated,
        RequestId::new(request_id)?,
        messages.to_vec(),
        tools,
        tool_choice,
        parallel_tools,
        RequestOptions::new(
            StructuredOutput::Text,
            reasoning,
            GenerationConfig::new(
                negotiated.limits().max_output_tokens().min(output_tokens),
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
    )?;
    Ok(if kind == ModelTurnKind::Developer {
        match &request.local_session_directory {
            Some(directory) => model_request.with_local_session_directory(directory.clone()),
            None => model_request,
        }
    } else {
        model_request
    })
}
