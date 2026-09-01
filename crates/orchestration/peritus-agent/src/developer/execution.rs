//! Tight provider/tool execution loop.

use peritus_model_protocol::{
    BoundedText, Capability, ContentBlock, Message, ModelRequest, ProtocolLimits, ReducedItem,
    RequestedCapabilities, Role, TerminalOutcome, ToolResult, negotiate,
};
use peritus_provider_core::ModelProvider;

use crate::{ModelAdvance, ModelSession};

use super::context::prepare_messages;
use super::model_request::{ModelTurnKind, build_model_request};
use super::observation::model_visible_tool_output;
use super::retry::DeveloperRetryPlanner;
use super::semantic::SemanticCompaction;
use super::{
    DeveloperLoopError, DeveloperLoopOutcome, DeveloperLoopRequest, DeveloperToolExecutor,
    DeveloperTrace, DeveloperTraceEvent, DeveloperUsage,
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
        let mut required_capabilities = vec![Capability::ToolCalls];
        if !request.attachments.is_empty() {
            required_capabilities.push(Capability::ImageInput);
        }
        let requested = RequestedCapabilities::new(
            &required_capabilities,
            &[
                Capability::Streaming,
                Capability::ParallelToolCalls,
                Capability::ReasoningControls,
                Capability::PromptCaching,
            ],
            profile.limits(),
        )?;
        let negotiated = negotiate(profile, requested)?;
        let mut messages = vec![
            message(Role::System, request.system.clone(), protocol_limits)?,
            user_message(request.prompt.clone(), request.attachments.clone(), protocol_limits)?,
        ];
        let mut tool_calls = 0_u32;
        let mut compactions = 0_u16;
        let mut retries = 0_u16;
        let mut usage = DeveloperUsage::default();

        for turn in 1..=request.limits.max_model_turns() {
            if request.cancellation.is_cancelled() {
                return Err(DeveloperLoopError::Cancelled);
            }
            if let Some(semantic) = SemanticCompaction::prepare(
                &messages,
                &request.tools,
                profile,
                request.limits.max_output_tokens(),
                protocol_limits,
            )? {
                match complete_turn(
                    provider,
                    &request,
                    semantic.request_messages(),
                    profile,
                    negotiated,
                    protocol_limits,
                    turn,
                    ModelTurnKind::SemanticCompaction,
                    &mut retries,
                    &mut usage,
                    trace,
                )
                .await
                {
                    Ok(session) => {
                        if let Ok(Some(record)) =
                            semantic.install(&mut messages, &session, protocol_limits)
                        {
                            trace.record(DeveloperTraceEvent::ContextCompaction(&record))?;
                            compactions = compactions
                                .checked_add(1)
                                .ok_or(DeveloperLoopError::LimitExceeded)?;
                        }
                    }
                    Err(DeveloperLoopError::Cancelled) => {
                        return Err(DeveloperLoopError::Cancelled);
                    }
                    Err(_) => {}
                }
            }
            let records = prepare_messages(
                &mut messages,
                &request.tools,
                profile,
                request.limits.max_output_tokens(),
                protocol_limits,
            )?;
            for record in &records {
                trace.record(DeveloperTraceEvent::ContextCompaction(record))?;
            }
            compactions = compactions
                .checked_add(
                    u16::try_from(records.len()).map_err(|_| DeveloperLoopError::LimitExceeded)?,
                )
                .ok_or(DeveloperLoopError::LimitExceeded)?;
            let session = complete_turn(
                provider,
                &request,
                &messages,
                profile,
                negotiated,
                protocol_limits,
                turn,
                ModelTurnKind::Developer,
                &mut retries,
                &mut usage,
                trace,
            )
            .await?;

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
                if let Some(blocker) = tools.completion_blocker() {
                    messages.push(Message::new(Role::Assistant, assistant, protocol_limits)?);
                    messages.push(message(
                        Role::User,
                        format!(
                            "The harness cannot accept that terminal response yet: {blocker}. Continue in this same session, use the declared host tools to satisfy the missing evidence, and then return the complete requested terminal response."
                        ),
                        protocol_limits,
                    )?);
                    continue;
                }
                return Ok(DeveloperLoopOutcome {
                    text: final_text,
                    model_turns: turn,
                    tool_calls,
                    compactions,
                    retries,
                    usage,
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
                let model_output = model_visible_tool_output(
                    &observation.output,
                    profile.limits().max_input_tokens(),
                    protocol_limits,
                )?;
                messages.push(Message::new(
                    Role::Tool,
                    vec![ContentBlock::ToolResult(ToolResult::new(
                        call.id().clone(),
                        model_output,
                        observation.is_error,
                    ))],
                    protocol_limits,
                )?);
            }
            if let Some(feedback) = tools.take_progress_feedback() {
                messages.push(message(Role::User, feedback, protocol_limits)?);
            }
        }
        Err(DeveloperLoopError::LimitExceeded)
    }
}

#[allow(clippy::too_many_arguments, reason = "one logical turn keeps its checked request inputs")]
async fn complete_turn(
    provider: &dyn ModelProvider,
    request: &DeveloperLoopRequest,
    messages: &[Message],
    profile: &peritus_model_protocol::ProviderProfile,
    negotiated: peritus_model_protocol::NegotiatedCapabilities,
    protocol_limits: ProtocolLimits,
    turn: u16,
    kind: ModelTurnKind,
    retries: &mut u16,
    usage: &mut DeveloperUsage,
    trace: &mut dyn DeveloperTrace,
) -> Result<ModelSession, DeveloperLoopError> {
    let maximum = request.limits.max_attempts_per_turn();
    let retry_prefix = match kind {
        ModelTurnKind::Developer => request.request_prefix.clone(),
        ModelTurnKind::SemanticCompaction => {
            format!("{}-semantic-compaction", request.request_prefix)
        }
    };
    let planner = DeveloperRetryPlanner::new(&retry_prefix, turn, maximum, &request.cancellation);
    for attempt in 1..=maximum {
        if request.cancellation.is_cancelled() {
            return Err(DeveloperLoopError::Cancelled);
        }
        let model_request = build_model_request(
            request,
            messages,
            profile,
            negotiated,
            protocol_limits,
            turn,
            attempt,
            kind,
        )?;
        match drive(provider, model_request, request, protocol_limits, trace).await {
            Ok(session) if successful(session.terminal()) && usable(&session) => {
                usage.observe(session.usage_high_water())?;
                return Ok(session);
            }
            Ok(session) => {
                usage.observe(session.usage_high_water())?;
                let Some(record) =
                    planner.terminal(attempt, session.terminal(), usable(&session))?
                else {
                    return Err(terminal_error(session.terminal()));
                };
                planner.record_and_wait(&record, trace).await?;
                *retries = retries.checked_add(1).ok_or(DeveloperLoopError::LimitExceeded)?;
            }
            Err(error) => {
                let Some(record) = planner.error(attempt, &error)? else {
                    return Err(error);
                };
                planner.record_and_wait(&record, trace).await?;
                *retries = retries.checked_add(1).ok_or(DeveloperLoopError::LimitExceeded)?;
            }
        }
    }
    Err(DeveloperLoopError::EmptyResponse)
}

async fn drive(
    provider: &dyn ModelProvider,
    model_request: ModelRequest,
    request: &DeveloperLoopRequest,
    protocol_limits: ProtocolLimits,
    trace: &mut dyn DeveloperTrace,
) -> Result<ModelSession, DeveloperLoopError> {
    let mut session =
        ModelSession::start(provider, model_request, protocol_limits, request.cancellation.clone())
            .await?;
    loop {
        match session.pull_one().await? {
            ModelAdvance::Closed => return Ok(session),
            ModelAdvance::EnvelopePending { .. } => {
                let encoded = session.encode_pending()?;
                trace.record(DeveloperTraceEvent::ProviderEnvelope(&encoded))?;
                let _ = session.accept_durable_pending()?;
            }
        }
    }
}

const fn successful(terminal: Option<&TerminalOutcome>) -> bool {
    matches!(
        terminal,
        Some(TerminalOutcome::Succeeded { .. } | TerminalOutcome::RequiresAction { .. })
    )
}

fn usable(session: &ModelSession) -> bool {
    session.completed_items().iter().any(|item| match item {
        ReducedItem::Text { text, .. } => !text.expose_for_wire().trim().is_empty(),
        ReducedItem::ToolCall { .. } | ReducedItem::Refusal { .. } => true,
        ReducedItem::Reasoning { .. }
        | ReducedItem::Structured { .. }
        | ReducedItem::ProviderNative { .. } => false,
    })
}

fn terminal_error(terminal: Option<&TerminalOutcome>) -> DeveloperLoopError {
    match terminal {
        Some(TerminalOutcome::Failed(failure)) => DeveloperLoopError::ProviderTerminal {
            provider: failure.provider().as_str().to_owned(),
            category: failure.category(),
            diagnostic_code: failure.diagnostic().code().to_owned(),
        },
        Some(TerminalOutcome::Refused { .. }) => DeveloperLoopError::Refused,
        Some(TerminalOutcome::Cancelled) => DeveloperLoopError::Cancelled,
        Some(
            TerminalOutcome::Succeeded { .. }
            | TerminalOutcome::RequiresAction { .. }
            | TerminalOutcome::Incomplete { .. },
        )
        | None => DeveloperLoopError::EmptyResponse,
    }
}

fn message(
    role: Role,
    value: String,
    limits: ProtocolLimits,
) -> Result<Message, DeveloperLoopError> {
    Ok(Message::new(role, vec![ContentBlock::Text(BoundedText::new(value, limits)?)], limits)?)
}

fn user_message(
    value: String,
    attachments: Vec<peritus_model_protocol::MediaInput>,
    limits: ProtocolLimits,
) -> Result<Message, DeveloperLoopError> {
    let mut content = Vec::with_capacity(attachments.len().saturating_add(1));
    content.push(ContentBlock::Text(BoundedText::new(value, limits)?));
    content.extend(attachments.into_iter().map(ContentBlock::Image));
    Ok(Message::new(Role::User, content, limits)?)
}
