//! Tight provider/tool execution loop.

use peritus_model_protocol::{
    BoundedText, CachePolicy, Capability, ContentBlock, GenerationConfig, Message, ModelRequest,
    ParallelToolPolicy, PersistencePolicy, ProtocolLimits, ReasoningPolicy, ReducedItem, RequestId,
    RequestOptions, RequestedCapabilities, Role, StructuredOutput, TerminalOutcome, ToolChoice,
    ToolResult, negotiate,
};
use peritus_provider_core::ModelProvider;

use crate::{ModelAdvance, ModelSession};

use super::{
    DeveloperLoopError, DeveloperLoopOutcome, DeveloperLoopRequest, DeveloperToolExecutor,
    DeveloperTrace, DeveloperTraceEvent,
};

/// Production D0 composition that repeatedly lets a model inspect, edit, execute, and observe.
pub struct DeveloperLoop;

impl DeveloperLoop {
    /// Runs a bounded developer loop until the provider returns final text without tool calls.
    ///
    /// Provider envelopes and tool observations are recorded through `trace` before they are
    /// admitted to subsequent model context.
    ///
    /// # Errors
    /// Returns typed protocol, provider, trace, tool, cancellation, or bound failures.
    #[allow(
        clippy::too_many_lines,
        reason = "the ordered provider and tool transcript remains explicit"
    )]
    pub async fn run(
        provider: &dyn ModelProvider,
        request: DeveloperLoopRequest,
        tools: &mut dyn DeveloperToolExecutor,
        trace: &mut dyn DeveloperTrace,
    ) -> Result<DeveloperLoopOutcome, DeveloperLoopError> {
        let protocol_limits = ProtocolLimits::PRODUCTION;
        let profile = provider.profile();
        let requested = RequestedCapabilities::new(
            &[Capability::ToolCalls],
            &[Capability::Streaming, Capability::ParallelToolCalls],
            profile.limits(),
        )?;
        let negotiated = negotiate(profile, requested)?;
        let mut messages = vec![
            message(Role::System, request.system.clone(), protocol_limits)?,
            message(Role::User, request.prompt.clone(), protocol_limits)?,
        ];
        let mut tool_calls = 0_u32;

        for turn in 1..=request.limits.max_model_turns() {
            if request.cancellation.is_cancelled() {
                return Err(DeveloperLoopError::Cancelled);
            }
            let model_request = ModelRequest::new(
                profile,
                negotiated,
                RequestId::new(format!("{}-{turn}", request.request_prefix))?,
                messages.clone(),
                request.tools.clone(),
                ToolChoice::Auto,
                ParallelToolPolicy::Disabled,
                RequestOptions::new(
                    StructuredOutput::Text,
                    ReasoningPolicy::Disabled,
                    GenerationConfig::new(
                        profile.limits().max_output_tokens().min(32_768),
                        Vec::new(),
                        None,
                        None,
                        None,
                    )?,
                    CachePolicy::Disabled,
                    PersistencePolicy::LOCAL_FIRST,
                    None,
                    Vec::new(),
                ),
                protocol_limits,
            )?;
            let mut session = ModelSession::start(
                provider,
                model_request,
                protocol_limits,
                request.cancellation.clone(),
            )
            .await?;
            loop {
                match session.pull_one().await? {
                    ModelAdvance::Closed => break,
                    ModelAdvance::EnvelopePending { .. } => {
                        let encoded = session.encode_pending()?;
                        trace.record(DeveloperTraceEvent::ProviderEnvelope(&encoded))?;
                        let _ = session.accept_durable_pending()?;
                    }
                }
            }
            let terminal = session.terminal().ok_or(DeveloperLoopError::EmptyResponse)?;
            if !matches!(
                terminal,
                TerminalOutcome::Succeeded { .. } | TerminalOutcome::RequiresAction { .. }
            ) {
                return Err(DeveloperLoopError::EmptyResponse);
            }

            let mut assistant = Vec::new();
            let mut calls = Vec::new();
            let mut final_text = String::new();
            for item in session.completed_items() {
                match item {
                    ReducedItem::Text { text, .. } => {
                        final_text.push_str(text.expose_for_wire());
                        assistant.push(ContentBlock::Text(text.clone()));
                    }
                    ReducedItem::ToolCall { call, .. } => {
                        calls.push(call.clone());
                        assistant.push(ContentBlock::ToolCall(call.clone()));
                    }
                    ReducedItem::Refusal { .. } => return Err(DeveloperLoopError::Refused),
                    ReducedItem::Reasoning { .. }
                    | ReducedItem::Structured { .. }
                    | ReducedItem::ProviderNative { .. } => {}
                }
            }
            if calls.is_empty() {
                if final_text.trim().is_empty() {
                    return Err(DeveloperLoopError::EmptyResponse);
                }
                return Ok(DeveloperLoopOutcome {
                    text: final_text,
                    model_turns: turn,
                    tool_calls,
                    messages,
                });
            }
            tool_calls = tool_calls
                .checked_add(
                    u32::try_from(calls.len()).map_err(|_| DeveloperLoopError::LimitExceeded)?,
                )
                .ok_or(DeveloperLoopError::LimitExceeded)?;
            if tool_calls > request.limits.max_tool_calls() {
                return Err(DeveloperLoopError::LimitExceeded);
            }
            if assistant.is_empty() {
                return Err(DeveloperLoopError::EmptyResponse);
            }
            messages.push(Message::new(Role::Assistant, assistant, protocol_limits)?);
            for call in calls {
                let observation = tools.execute(&call)?;
                trace.record(DeveloperTraceEvent::ToolObservation {
                    call: &call,
                    observation: &observation,
                })?;
                messages.push(Message::new(
                    Role::Tool,
                    vec![ContentBlock::ToolResult(ToolResult::new(
                        call.id().clone(),
                        observation.output,
                        observation.is_error,
                    ))],
                    protocol_limits,
                )?);
            }
        }
        Err(DeveloperLoopError::LimitExceeded)
    }
}

fn message(
    role: Role,
    value: String,
    limits: ProtocolLimits,
) -> Result<Message, DeveloperLoopError> {
    Ok(Message::new(role, vec![ContentBlock::Text(BoundedText::new(value, limits)?)], limits)?)
}
