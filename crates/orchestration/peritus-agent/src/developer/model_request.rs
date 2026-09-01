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
) -> Result<ModelRequest, DeveloperLoopError> {
    let parallel_tools =
        if kind == ModelTurnKind::Developer && negotiated.includes(Capability::ParallelToolCalls) {
            ParallelToolPolicy::Allowed(negotiated.limits().max_parallel_tool_calls())
        } else {
            ParallelToolPolicy::Disabled
        };
    let reasoning = if negotiated.includes(Capability::ReasoningControls) {
        ReasoningPolicy::Effort { effort: ReasoningEffort::High, summary: SummaryPolicy::None }
    } else {
        ReasoningPolicy::Disabled
    };
    let (request_id, tools, tool_choice, output_tokens) = match kind {
        ModelTurnKind::Developer => (
            format!("{}-{turn}-attempt-{attempt}", request.request_prefix),
            request.tools.clone(),
            ToolChoice::Auto,
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
        messages.to_vec(),
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
