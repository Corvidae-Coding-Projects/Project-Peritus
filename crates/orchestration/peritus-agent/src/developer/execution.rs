//! Tight provider/tool execution loop.
mod invocation;
mod provider_turn;
use provider_turn::{RetryContext, complete_turn};

use peritus_model_protocol::{
    CanonicalJson, Capability, CompletedToolCall, ContentBlock, JsonBounds, MediaInput, Message,
    ProtocolLimits, ProviderProfile, RequestedCapabilities, Role, ToolResult, encode_messages,
    negotiate,
};
use peritus_provider_core::ModelProvider;

use super::model_request::ModelTurnKind;
use super::observation::model_visible_tool_output;
use super::semantic::SemanticCompaction;
use super::{
    DeveloperAccountingEvent, DeveloperActivity, DeveloperCompactionOwner,
    DeveloperContextEvent, DeveloperInteraction, DeveloperLoop, DeveloperLoopError,
    DeveloperLoopOutcome, DeveloperLoopProgress, DeveloperLoopRequest, DeveloperModelRole,
    DeveloperToolExecutor,
    DeveloperToolObservation, DeveloperTrace, DeveloperTraceEvent, DeveloperUsage,
};
use super::{context::prepare_messages, context_port::ContextSession};

impl DeveloperLoop {
    #[allow(
        clippy::too_many_lines,
        reason = "the ordered provider and tool transcript remains explicit"
    )]
    pub(super) async fn run_inner(
        provider: &dyn ModelProvider,
        mut request: DeveloperLoopRequest,
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
        let compaction_owner = context.negotiate_compaction(interaction)?;
        let resume = context.resume(&request, &messages)?;
        if resume.is_none() {
            context.open(&request, &messages)?;
        }
        let mut first_turn = 1;
        let mut tool_calls = 0;
        let mut retries = 0;
        let mut segment_progress = DeveloperLoopProgress::default();
        let mut pending_exchange = Vec::new();
        let mut pending_calls = Vec::new();
        let mut pending_first_sequence = 1;
        let mut resumed_turn = resume.is_some();
        let mut resumed_segment =
            resume.as_ref().is_some_and(|resume| resume.is_segment_continuation());
        if let Some(resume) = resume {
            first_turn = resume.turn();
            tool_calls = resume.tool_calls();
            retries = resume.retries();
            segment_progress = resume.progress();
            let (
                system,
                prompt,
                mut attachments,
                mut resumed_messages,
                resumed_exchange,
                resumed_calls,
                resumed_first_sequence,
            ) =
                resume.into_request_state();
            restore_resolved_resume_media(
                &messages,
                &system,
                &prompt,
                &mut attachments,
                &mut resumed_messages,
                protocol_limits,
                !resumed_segment,
            )?;
            request.system = system;
            request.prompt = prompt;
            request.attachments = attachments;
            messages = resumed_messages;
            pending_exchange = resumed_exchange;
            pending_calls = resumed_calls;
            pending_first_sequence = resumed_first_sequence;
        }
        let execution_prefix = request.limits.segment_request_prefix(&request.request_prefix);
        let mut completed_model_turns = first_turn.saturating_sub(1);
        let mut compactions = 0_u64;
        let mut usage = DeveloperUsage::default();
        let mut input_revision = 0;
        let mut governing_installed = false;

        if first_turn > request.limits.max_model_turns() {
            return Err(DeveloperLoopError::SegmentExhausted);
        }
        for turn in first_turn..=request.limits.max_model_turns() {
            if request.cancellation.is_cancelled() {
                return Err(DeveloperLoopError::Cancelled);
            }
            let selected = live
                .map(|(port, role)| port.provider_selection(role))
                .transpose()?
                .flatten();
            let provider_selection = selected.as_ref().and_then(|selected| selected.provenance());
            let provider = selected.as_ref().map_or(provider, |selected| selected.provider());
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
                    Capability::ReasoningSummaries,
                    Capability::ReasoningReplay,
                    Capability::PromptCaching,
                ],
                profile.limits(),
            )?;
            let negotiated = negotiate(profile, requested)?;
            tools.observe_provider_profile(profile)?;
            let reserved_output_tokens = request.limits.max_output_tokens()
                .min(negotiated.limits().max_output_tokens());
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
            if !pending_exchange.is_empty() {
                messages.append(&mut pending_exchange);
                execute_tool_batch(
                    &request,
                    tools,
                    trace,
                    &mut context,
                    live,
                    interaction,
                    protocol_limits,
                    profile,
                    &execution_prefix,
                    input_revision,
                    &mut messages,
                    std::mem::take(&mut pending_calls),
                    pending_first_sequence,
                )
                .await?;
                if tools.yields_to_host() {
                    let progress = segment_progress.checked_add_segment(
                        completed_model_turns,
                        tool_calls,
                        compactions,
                        retries,
                    )?;
                    return Ok(DeveloperLoopOutcome {
                        text: String::new(),
                        model_turns: progress.model_turns(),
                        tool_calls: progress.tool_calls(),
                        compactions: progress.compactions(),
                        retries: progress.retries(),
                        usage,
                        messages,
                    });
                }
                if tool_calls >= request.limits.max_tool_calls() {
                    let next_segment = request
                        .limits
                        .segment_sequence()
                        .checked_add(1)
                        .ok_or(DeveloperLoopError::LimitExceeded)?;
                    let progress = segment_progress.checked_add_segment(
                        completed_model_turns,
                        tool_calls,
                        compactions,
                        retries,
                    )?;
                    return if request.limits.permits_segment_continuation()
                        && context.schedule_segment(next_segment, progress, &[])?
                    {
                        Err(DeveloperLoopError::SegmentContinuation)
                    } else {
                        Err(DeveloperLoopError::SegmentExhausted)
                    };
                }
            }
            let required_tool = tools.required_tool_name().map(str::to_owned);
            let invocation_policy = invocation::policy(&request, turn, required_tool.as_deref())?;
            if compaction_owner == DeveloperCompactionOwner::LocalContext {
                if resumed_turn {
                    resumed_turn = false;
                    if resumed_segment {
                        resumed_segment = false;
                        if context.prepare(
                            &mut messages,
                            &request.tools,
                            profile,
                            &invocation_policy,
                            governing_input.as_ref(),
                        )? {
                            trace.account(DeveloperAccountingEvent::Compaction)?;
                            compactions = compactions
                                .checked_add(1)
                                .ok_or(DeveloperLoopError::LimitExceeded)?;
                        }
                    } else {
                        let governing_matches = if let Some(input) = governing_input.as_ref() {
                            restore_exact_message_media(input, &mut messages, protocol_limits)?
                        } else {
                            true
                        };
                        if messages.first() != Some(&invocation_policy)
                            || !governing_matches
                        {
                            return Err(DeveloperLoopError::RecoveryRequired(
                                "durable retry context changed before exact request reentry"
                                    .to_owned(),
                            ));
                        }
                    }
                } else if context.prepare(
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
                if let Some(input) = governing_input.as_ref() {
                    if governing_installed {
                        messages[2] = input.clone();
                    } else {
                        messages.insert(2, input.clone());
                        governing_installed = true;
                    }
                }
                let protected_prefix = if governing_installed { 3 } else { 2 };
                if compaction_owner.permits_provider_semantic()
                    && let Some(semantic) = SemanticCompaction::prepare(
                        &messages,
                        &request.tools,
                        profile,
                        reserved_output_tokens,
                        protocol_limits,
                        protected_prefix,
                    )?
                {
                    let semantic_messages = semantic.request_messages().to_vec();
                    match complete_turn(
                        provider,
                        &request,
                        &semantic_messages,
                        profile,
                        negotiated,
                        protocol_limits,
                        turn,
                        ModelTurnKind::SemanticCompaction,
                        None,
                        0,
                        &mut retries,
                        &mut usage,
                        trace,
                        None,
                        None,
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
                    reserved_output_tokens,
                    protocol_limits,
                    protected_prefix,
                )?;
                for record in &records {
                    trace.record(DeveloperTraceEvent::ContextCompaction(record))?;
                    trace.account(DeveloperAccountingEvent::Compaction)?;
                }
                compactions = compactions
                    .checked_add(
                        u64::try_from(records.len())
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
                request
                    .limits
                    .max_tool_calls()
                    .checked_sub(tool_calls)
                    .ok_or(DeveloperLoopError::LimitExceeded)?,
                &mut retries,
                &mut usage,
                trace,
                provider_selection,
                live.map(|(port, role)| (port, role, input_revision)),
                Some(RetryContext {
                    context: &mut context,
                    tools,
                }),
            )
            .await?
            else {
                continue;
            };
            completed_model_turns = turn;

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
                if compaction_owner == DeveloperCompactionOwner::LocalContext {
                    context.observe(DeveloperContextEvent::Message(&Message::new(
                        Role::Assistant,
                        assistant,
                        protocol_limits,
                    )?))?;
                }
                context.complete_invocation()?;
                let progress = segment_progress.checked_add_segment(
                    completed_model_turns,
                    tool_calls,
                    compactions,
                    retries,
                )?;
                return Ok(DeveloperLoopOutcome {
                    text: final_text,
                    model_turns: progress.model_turns(),
                    tool_calls: progress.tool_calls(),
                    compactions: progress.compactions(),
                    retries: progress.retries(),
                    usage,
                    messages,
                });
            }
            context.append(
                &mut messages,
                Message::new(Role::Assistant, assistant, protocol_limits)?,
            )?;
            let calls_in_batch =
                u32::try_from(calls.len()).map_err(|_| DeveloperLoopError::LimitExceeded)?;
            let remaining_tool_calls = request
                .limits
                .max_tool_calls()
                .checked_sub(tool_calls)
                .ok_or(DeveloperLoopError::LimitExceeded)?;
            if calls_in_batch > remaining_tool_calls {
                if calls_in_batch > request.limits.max_tool_calls() {
                    return Err(DeveloperLoopError::LimitExceeded);
                }
                let next_segment = request
                    .limits
                    .segment_sequence()
                    .checked_add(1)
                    .ok_or(DeveloperLoopError::LimitExceeded)?;
                let progress = segment_progress.checked_add_segment(
                    completed_model_turns,
                    tool_calls,
                    compactions,
                    retries,
                )?;
                return if request.limits.permits_segment_continuation()
                    && context.schedule_segment(next_segment, progress, &calls)?
                {
                    Err(DeveloperLoopError::SegmentContinuation)
                } else {
                    Err(DeveloperLoopError::SegmentExhausted)
                };
            }
            tool_calls = tool_calls
                .checked_add(calls_in_batch)
                .ok_or(DeveloperLoopError::LimitExceeded)?;
            let first_sequence = tool_calls
                .checked_sub(calls_in_batch)
                .and_then(|value| value.checked_add(1))
                .ok_or(DeveloperLoopError::LimitExceeded)?;
            execute_tool_batch(
                &request,
                tools,
                trace,
                &mut context,
                live,
                interaction,
                protocol_limits,
                profile,
                &execution_prefix,
                input_revision,
                &mut messages,
                calls,
                first_sequence,
            )
            .await?;
            if tools.yields_to_host() {
                let progress = segment_progress.checked_add_segment(
                    completed_model_turns,
                    tool_calls,
                    compactions,
                    retries,
                )?;
                return Ok(DeveloperLoopOutcome {
                    text: String::new(),
                    model_turns: progress.model_turns(),
                    tool_calls: progress.tool_calls(),
                    compactions: progress.compactions(),
                    retries: progress.retries(),
                    usage,
                    messages,
                });
            }
            if request.limits.permits_segment_continuation()
                && tool_calls >= request.limits.max_tool_calls()
            {
                let next_segment = request
                    .limits
                    .segment_sequence()
                    .checked_add(1)
                    .ok_or(DeveloperLoopError::LimitExceeded)?;
                let progress = segment_progress.checked_add_segment(
                    completed_model_turns,
                    tool_calls,
                    compactions,
                    retries,
                )?;
                return if context.schedule_segment(next_segment, progress, &[])? {
                    Err(DeveloperLoopError::SegmentContinuation)
                } else {
                    Err(DeveloperLoopError::SegmentExhausted)
                };
            }
        }
        if request.limits.permits_segment_continuation() {
            let next_segment = request
                .limits
                .segment_sequence()
                .checked_add(1)
                .ok_or(DeveloperLoopError::LimitExceeded)?;
            let progress = segment_progress.checked_add_segment(
                completed_model_turns,
                tool_calls,
                compactions,
                retries,
            )?;
            if context.schedule_segment(next_segment, progress, &[])? {
                return Err(DeveloperLoopError::SegmentContinuation);
            }
        }
        Err(DeveloperLoopError::SegmentExhausted)
    }
}

#[allow(clippy::too_many_arguments, reason = "one atomic tool batch retains every host boundary")]
async fn execute_tool_batch(
    request: &DeveloperLoopRequest,
    tools: &mut dyn DeveloperToolExecutor,
    trace: &mut dyn DeveloperTrace,
    context: &mut ContextSession<'_>,
    live: Option<(&dyn DeveloperInteraction, DeveloperModelRole)>,
    interaction: Option<&dyn DeveloperInteraction>,
    protocol_limits: ProtocolLimits,
    profile: &ProviderProfile,
    execution_prefix: &str,
    input_revision: u64,
    messages: &mut Vec<Message>,
    calls: Vec<CompletedToolCall>,
    first_sequence: u32,
) -> Result<(), DeveloperLoopError> {
    for (index, call) in calls.into_iter().enumerate() {
        if request.cancellation.is_cancelled() {
            return Err(DeveloperLoopError::Cancelled);
        }
        let name = call.name().as_str();
        let sequence = first_sequence
            .checked_add(u32::try_from(index).map_err(|_| DeveloperLoopError::LimitExceeded)?)
            .ok_or(DeveloperLoopError::LimitExceeded)?;
        let mut yielded = tools.yields_to_host() || input_changed(interaction, input_revision)?;
        if !yielded && let Some((port, role)) = live {
            match port.admit_tool(
                role,
                execution_prefix,
                sequence,
                input_revision,
                tools.effect(&call),
            )? {
                crate::DeveloperControlFlow::Continue => {}
                crate::DeveloperControlFlow::Yield => yielded = true,
                crate::DeveloperControlFlow::Stop => {
                    port.observe(DeveloperActivity::ToolSkipped { name })?;
                    return Err(DeveloperLoopError::Cancelled);
                }
            }
        }
        let observation = if yielded {
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
            if let Some(port) = interaction {
                port.observe(DeveloperActivity::ToolStarted {
                    name,
                    arguments: &call.arguments().to_wire_string(),
                })?;
            }
            let observation = tools.execute_async(&call).await?;
            if request.cancellation.is_cancelled() {
                return Err(DeveloperLoopError::Cancelled);
            }
            trace.account(DeveloperAccountingEvent::ToolCall)?;
            if let Some(port) = interaction {
                port.observe(DeveloperActivity::ToolFinished {
                    name,
                    output: &observation.output.to_wire_string(),
                    is_error: observation.is_error,
                })?;
            }
            if let Some((port, role)) = live
                && port.complete_tool(role, execution_prefix, sequence)?
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
            call.name().as_str(),
            &observation.output,
            profile.limits().max_input_tokens(),
            protocol_limits,
        )?;
        let model_output = context.annotate(&call, model_output)?;
        context.append(
            messages,
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
    let continuation_blocker = tools.continuation_blocker();
    if let Some(feedback) = tools.take_progress_feedback() {
        context.append(messages, message(Role::User, feedback, protocol_limits)?)?;
    }
    context.observe(DeveloperContextEvent::BatchCompleted)?;
    if let Some(blocker) = continuation_blocker {
        return Err(DeveloperLoopError::Tool(blocker));
    }
    Ok(())
}

mod state;
use state::{input_changed, message, successful, terminal_error, usable, user_message};

fn restore_resolved_resume_media(
    current_inputs: &[Message],
    resumed_system: &str,
    resumed_prompt: &str,
    resumed_attachments: &mut Vec<MediaInput>,
    resumed_messages: &mut [Message],
    limits: ProtocolLimits,
    require_exact_inputs: bool,
) -> Result<(), DeveloperLoopError> {
    let resumed_inputs = [
        message(Role::System, resumed_system.to_owned(), limits)?,
        user_message(
            resumed_prompt.to_owned(),
            resumed_attachments.clone(),
            limits,
        )?,
    ];
    if require_exact_inputs
        && encode_messages(current_inputs, limits)? != encode_messages(&resumed_inputs, limits)?
    {
        return Err(DeveloperLoopError::RecoveryRequired(
            "durable retry inputs no longer match the current artifact identities".to_owned(),
        ));
    }
    if !require_exact_inputs {
        return Ok(());
    }
    let [_, current_user] = current_inputs else {
        return Err(DeveloperLoopError::RecoveryRequired(
            "current retry inputs changed shape".to_owned(),
        ));
    };
    let Some((ContentBlock::Text(_), current_media)) = current_user.content().split_first() else {
        return Err(DeveloperLoopError::RecoveryRequired(
            "current retry attachment input changed shape".to_owned(),
        ));
    };
    let current_media = current_media
        .iter()
        .map(|block| match block {
            ContentBlock::Image(media) => Ok(media.clone()),
            _ => Err(DeveloperLoopError::RecoveryRequired(
                "current retry attachment input changed shape".to_owned(),
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    *resumed_attachments = current_media;

    let restored = restore_exact_message_media(current_user, resumed_messages, limits)?;
    if !resumed_attachments.is_empty() && !restored {
        return Err(DeveloperLoopError::RecoveryRequired(
            "durable retry view no longer contains its exact artifact input".to_owned(),
        ));
    }
    Ok(())
}

fn restore_exact_message_media(
    current: &Message,
    recovered: &mut [Message],
    limits: ProtocolLimits,
) -> Result<bool, DeveloperLoopError> {
    let current_bytes = encode_messages(std::slice::from_ref(current), limits)?;
    let mut restored = false;
    for message in recovered {
        if encode_messages(std::slice::from_ref(message), limits)? == current_bytes {
            *message = current.clone();
            restored = true;
        }
    }
    Ok(restored)
}
