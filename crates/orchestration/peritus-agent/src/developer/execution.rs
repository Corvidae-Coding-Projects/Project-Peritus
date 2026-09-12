//! Tight provider/tool execution loop.
mod invocation;
mod provider_turn;
use provider_turn::complete_turn;

use peritus_model_protocol::{
    CanonicalJson, Capability, ContentBlock, JsonBounds, Message, ProtocolLimits,
    RequestedCapabilities, Role, ToolResult, negotiate,
};
use peritus_provider_core::ModelProvider;

use super::context::prepare_messages;
use super::context_port::ContextSession;
use super::model_request::ModelTurnKind;
use super::observation::model_visible_tool_output;
use super::semantic::SemanticCompaction;
use super::{
    DeveloperAccountingEvent, DeveloperActivity, DeveloperContextEvent, DeveloperInteraction,
    DeveloperLoop, DeveloperLoopError, DeveloperLoopOutcome, DeveloperLoopRequest,
    DeveloperToolExecutor, DeveloperToolObservation, DeveloperTrace, DeveloperTraceEvent,
    DeveloperUsage,
};

impl DeveloperLoop {
    #[allow(
        clippy::too_many_lines,
        reason = "the ordered provider and tool transcript remains explicit"
    )]
    pub(super) async fn run_inner(
        provider: &dyn ModelProvider,
        request: DeveloperLoopRequest,
        tools: &mut dyn DeveloperToolExecutor,
        trace: &mut dyn DeveloperTrace,
        mut context: ContextSession<'_>,
        live: Option<(&dyn DeveloperInteraction, super::DeveloperModelRole)>,
    ) -> Result<DeveloperLoopOutcome, DeveloperLoopError> {
        let protocol_limits = ProtocolLimits::PRODUCTION;
        let interaction = live.map(|(port, _)| port);
        let mut messages = vec![
            message(Role::System, request.system.clone(), protocol_limits)?,
            user_message(request.prompt.clone(), request.attachments.clone(), protocol_limits)?,
        ];
        context.open(&request, &messages)?;
        let mut tool_calls = 0_u32;
        let mut compactions = 0_u16;
        let mut retries = 0_u16;
        let mut usage = DeveloperUsage::default();
        let mut input_revision = 0;
        let mut governing_installed = false;

        for turn in 1..=request.limits.max_model_turns() {
            if request.cancellation.is_cancelled() {
                return Err(DeveloperLoopError::Cancelled);
            }
            let selected = live.map(|(port, role)| port.provider(role)).transpose()?.flatten();
            let provider = selected.as_deref().unwrap_or(provider);
            let profile = provider.profile();
            let input = interaction.map(DeveloperInteraction::input).transpose()?;
            let mut required_capabilities = vec![Capability::ToolCalls];
            if !request.attachments.is_empty()
                || input.as_ref().is_some_and(|input| !input.images.is_empty())
            {
                required_capabilities.push(Capability::ImageInput);
            }
            let requested = RequestedCapabilities::new(
                &required_capabilities,
                &[
                    Capability::Streaming,
                    Capability::UsageDetail,
                    Capability::ParallelToolCalls,
                    Capability::ReasoningControls,
                    Capability::ReasoningReplay,
                    Capability::PromptCaching,
                ],
                profile.limits(),
            )?;
            let negotiated = negotiate(profile, requested)?;
            let governing_input = if let Some(input) = input {
                input_revision = input.revision;
                Some(user_message(
                    format!(
                        "Current governing conversation (revision {}); incorporate the latest user message and respect its scope:\n{}",
                        input.revision, input.conversation
                    ),
                    input.images,
                    protocol_limits,
                )?)
            } else {
                None
            };
            let required_tool = tools.required_tool_name().map(str::to_owned);
            let invocation_policy = invocation::policy(&request, turn, required_tool.as_deref())?;
            if context.is_local() {
                if context.prepare(
                    &mut messages,
                    &request.tools,
                    profile,
                    &invocation_policy,
                    governing_input.as_ref(),
                )? {
                    trace.account(DeveloperAccountingEvent::Compaction)?;
                    compactions =
                        compactions.checked_add(1).ok_or(DeveloperLoopError::LimitExceeded)?;
                }
            } else {
                // Replace, never append: the current host projection is budgeted and cannot
                // accumulate stale startup states or be compacted as optional conversation.
                messages[0] = invocation_policy;
                if let Some(input) = governing_input {
                    if governing_installed {
                        messages[2] = input;
                    } else {
                        messages.insert(2, input);
                        governing_installed = true;
                    }
                }
                let protected_prefix = if governing_installed { 3 } else { 2 };
                if interaction.is_none_or(DeveloperInteraction::allows_semantic_compaction)
                    && let Some(semantic) = SemanticCompaction::prepare(
                        &messages,
                        &request.tools,
                        profile,
                        request.limits.max_output_tokens(),
                        protocol_limits,
                        protected_prefix,
                    )?
                {
                    match complete_turn(
                        provider,
                        &request,
                        semantic.request_messages(),
                        profile,
                        negotiated,
                        protocol_limits,
                        turn,
                        ModelTurnKind::SemanticCompaction,
                        None,
                        &mut retries,
                        &mut usage,
                        trace,
                        None,
                    )
                    .await
                    {
                        Ok(Some(session)) => {
                            if let Ok(Some(record)) =
                                semantic.install(&mut messages, &session, protocol_limits)
                            {
                                trace.record(DeveloperTraceEvent::ContextCompaction(&record))?;
                                trace.account(DeveloperAccountingEvent::Compaction)?;
                                compactions = compactions
                                    .checked_add(1)
                                    .ok_or(DeveloperLoopError::LimitExceeded)?;
                            }
                        }
                        Err(DeveloperLoopError::Cancelled) => {
                            return Err(DeveloperLoopError::Cancelled);
                        }
                        Ok(None) | Err(_) => {}
                    }
                }
                let records = prepare_messages(
                    &mut messages,
                    &request.tools,
                    profile,
                    request.limits.max_output_tokens(),
                    protocol_limits,
                    protected_prefix,
                )?;
                for record in &records {
                    trace.record(DeveloperTraceEvent::ContextCompaction(record))?;
                    trace.account(DeveloperAccountingEvent::Compaction)?;
                }
                compactions = compactions
                    .checked_add(
                        u16::try_from(records.len())
                            .map_err(|_| DeveloperLoopError::LimitExceeded)?,
                    )
                    .ok_or(DeveloperLoopError::LimitExceeded)?;
            }
            let Some(session) = complete_turn(
                provider,
                &request,
                &messages,
                profile,
                negotiated,
                protocol_limits,
                turn,
                ModelTurnKind::Developer,
                required_tool.as_deref(),
                &mut retries,
                &mut usage,
                trace,
                live.map(|(port, role)| (port, role, input_revision)),
            )
            .await?
            else {
                continue;
            };

            let (assistant, calls, final_text) =
                state::assistant_items(session.completed_items(), protocol_limits)?;
            if calls.is_empty() {
                if final_text.trim().is_empty() {
                    return Err(DeveloperLoopError::EmptyResponse);
                }
                if input_changed(interaction, input_revision)? {
                    context.append(
                        &mut messages,
                        Message::new(Role::Assistant, assistant, protocol_limits)?,
                    )?;
                    continue;
                }
                if let Some(blocker) = tools.completion_blocker() {
                    context.append(
                        &mut messages,
                        Message::new(Role::Assistant, assistant, protocol_limits)?,
                    )?;
                    context.append(&mut messages, message(
                        Role::User,
                        format!(
                            "The harness cannot accept that terminal response yet: {blocker}. Continue in this same session, use the declared host tools to satisfy the missing evidence, and then return the complete requested terminal response."
                        ),
                        protocol_limits,
                    )?)?;
                    continue;
                }
                if context.is_local() {
                    context.observe(DeveloperContextEvent::Message(&Message::new(
                        Role::Assistant,
                        assistant,
                        protocol_limits,
                    )?))?;
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
            context.append(
                &mut messages,
                Message::new(Role::Assistant, assistant, protocol_limits)?,
            )?;
            tool_calls = tool_calls
                .checked_add(
                    u32::try_from(calls.len()).map_err(|_| DeveloperLoopError::LimitExceeded)?,
                )
                .ok_or(DeveloperLoopError::LimitExceeded)?;
            if tool_calls > request.limits.max_tool_calls() {
                return Err(DeveloperLoopError::LimitExceeded);
            }
            let calls_in_batch =
                u32::try_from(calls.len()).map_err(|_| DeveloperLoopError::LimitExceeded)?;
            let first_sequence = tool_calls
                .checked_sub(calls_in_batch)
                .and_then(|value| value.checked_add(1))
                .ok_or(DeveloperLoopError::LimitExceeded)?;
            for (index, call) in calls.into_iter().enumerate() {
                if request.cancellation.is_cancelled() {
                    return Err(DeveloperLoopError::Cancelled);
                }
                let name = call.name().as_str();
                let sequence = first_sequence
                    .checked_add(
                        u32::try_from(index).map_err(|_| DeveloperLoopError::LimitExceeded)?,
                    )
                    .ok_or(DeveloperLoopError::LimitExceeded)?;
                let observation = if tools.yields_to_host()
                    || input_changed(interaction, input_revision)?
                {
                    if let Some(port) = interaction {
                        port.observe(DeveloperActivity::ToolSkipped { name })?;
                    }
                    DeveloperToolObservation {
                        output: CanonicalJson::parse(
                            r#"{"error":"Not executed: control was handed to the host or a newer user message superseded this tool call."}"#,
                            JsonBounds::value(protocol_limits),
                        )?,
                        is_error: true,
                    }
                } else {
                    if let Some((port, role)) = live
                        && port.admit_tool(
                            role,
                            &request.request_prefix,
                            sequence,
                            tools.effect(&call),
                        )? == crate::DeveloperControlFlow::Stop
                    {
                        port.observe(DeveloperActivity::ToolSkipped { name })?;
                        return Err(DeveloperLoopError::Cancelled);
                    }
                    if let Some(port) = interaction {
                        port.observe(DeveloperActivity::ToolStarted {
                            name,
                            arguments: &call.arguments().to_wire_string(),
                        })?;
                    }
                    let observation = tools.execute(&call)?;
                    trace.account(DeveloperAccountingEvent::ToolCall)?;
                    if let Some(port) = interaction {
                        port.observe(DeveloperActivity::ToolFinished {
                            name,
                            output: &observation.output.to_wire_string(),
                            is_error: observation.is_error,
                        })?;
                    }
                    if let Some((port, role)) = live
                        && port.complete_tool(role, &request.request_prefix, sequence)?
                            == crate::DeveloperControlFlow::Stop
                    {
                        trace.record(DeveloperTraceEvent::ToolObservation {
                            call: &call,
                            observation: &observation,
                        })?;
                        context.observe(DeveloperContextEvent::ToolObservation {
                            call: &call,
                            observation: &observation,
                        })?;
                        return Err(DeveloperLoopError::Cancelled);
                    }
                    observation
                };
                trace.record(DeveloperTraceEvent::ToolObservation {
                    call: &call,
                    observation: &observation,
                })?;
                context.observe(DeveloperContextEvent::ToolObservation {
                    call: &call,
                    observation: &observation,
                })?;
                let model_output = model_visible_tool_output(
                    &observation.output,
                    profile.limits().max_input_tokens(),
                    protocol_limits,
                )?;
                let model_output = context.annotate(&call, model_output)?;
                context.append(
                    &mut messages,
                    Message::new(
                        Role::Tool,
                        vec![ContentBlock::ToolResult(ToolResult::new(
                            call.id().clone(),
                            model_output,
                            observation.is_error,
                        ))],
                        protocol_limits,
                    )?,
                )?;
            }
            if let Some(feedback) = tools.take_progress_feedback() {
                context.append(&mut messages, message(Role::User, feedback, protocol_limits)?)?;
            }
            context.observe(DeveloperContextEvent::BatchCompleted)?;
            if let Some(blocker) = tools.continuation_blocker() {
                return Err(DeveloperLoopError::Tool(blocker));
            }
            if tools.yields_to_host() {
                return Ok(DeveloperLoopOutcome {
                    text: String::new(),
                    model_turns: turn,
                    tool_calls,
                    compactions,
                    retries,
                    usage,
                    messages,
                });
            }
        }
        Err(DeveloperLoopError::LimitExceeded)
    }
}

mod state;
use state::{input_changed, message, successful, terminal_error, usable, user_message};
