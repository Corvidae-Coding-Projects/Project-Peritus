//! Acceptance-safe provider reconnects and durable public-text delivery.
use super::super::{
    DeveloperAccountingEvent, DeveloperActivity, DeveloperControlFlow, DeveloperInteraction,
    DeveloperLoopError, DeveloperLoopRequest, DeveloperModelRole, DeveloperRequestAdmission,
    DeveloperProviderRequestIdentity, DeveloperRetryDisposition, DeveloperRetryRecovery,
    DeveloperToolExecutor, DeveloperTrace, DeveloperTraceEvent, DeveloperUsage,
    model_request::{ModelTurnKind, build_model_request},
    retry::{DeveloperRetryPlanner, native_session_digest, wait_until_eligible},
};
use super::{ContextSession, successful, terminal_error, usable};
use crate::{ModelAdvance, ModelSession};
use peritus_model_protocol::{
    Message, ModelEvent, ModelFailure, ModelRequest, OutcomeCertainty, ProtocolLimits,
    TerminalOutcome,
};
use peritus_provider_core::{
    CancellationToken, ModelProvider, ProviderCoreErrorKind as CoreErrorKind, cancel_first,
};

mod progress;

pub(super) struct RetryContext<'a, 'port> {
    pub(super) context: &'a mut ContextSession<'port>,
    pub(super) tools: &'a mut dyn DeveloperToolExecutor,
}

enum DriveOutcome {
    Terminal(ModelSession),
    Rejected { session: ModelSession, failure: ModelFailure },
}

impl DriveOutcome {
    const fn session(&self) -> &ModelSession {
        match self {
            Self::Terminal(session) | Self::Rejected { session, .. } => session,
        }
    }
}

impl RetryContext<'_, '_> {
    fn bind_session(
        owner: Option<&Self>,
        kind: ModelTurnKind,
        request: ModelRequest,
    ) -> ModelRequest {
        if kind == ModelTurnKind::Developer
            && let Some(directory) = owner.and_then(|owner| owner.context.local_session_directory())
        {
            return request.with_local_session_directory(directory);
        }
        request
    }
}

#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "one logical turn keeps its checked request inputs and acceptance boundaries"
)]
pub(super) async fn complete_turn(
    provider: &dyn ModelProvider,
    request: &DeveloperLoopRequest,
    messages: &[Message],
    profile: &peritus_model_protocol::ProviderProfile,
    negotiated: peritus_model_protocol::NegotiatedCapabilities,
    protocol_limits: ProtocolLimits,
    turn: u16,
    kind: ModelTurnKind,
    required_tool: Option<&str>,
    remaining_tool_calls: u32,
    retries: &mut u64,
    usage: &mut DeveloperUsage,
    trace: &mut dyn DeveloperTrace,
    provider_selection: Option<peritus_types::Sha256Digest>,
    interaction: Option<(&dyn DeveloperInteraction, DeveloperModelRole, u64)>,
    mut retry_context: Option<RetryContext<'_, '_>>,
) -> Result<Option<ModelSession>, DeveloperLoopError> {
    let segment_prefix = request.limits.segment_request_prefix(&request.request_prefix);
    let retry_prefix = match kind {
        ModelTurnKind::Developer => segment_prefix.clone(),
        ModelTurnKind::SemanticCompaction => {
            format!("{segment_prefix}-semantic-compaction")
        }
    };
    let selection_superseded = match provider_selection {
        Some(provider_selection) => trace.supersede_retry_for_provider_selection(
            &retry_prefix,
            turn,
            provider_selection,
        )?,
        None => false,
    };
    let recovery_probe = build_model_request(
        request,
        messages,
        profile,
        negotiated,
        protocol_limits,
        turn,
        1,
        kind,
        required_tool,
        provider.reasoning_effort(),
        remaining_tool_calls,
    )?;
    let recovery_probe = RetryContext::bind_session(retry_context.as_ref(), kind, recovery_probe);
    provider.validate_request(&recovery_probe)?;
    let recovered = if selection_superseded {
        None
    } else {
        trace.recover_retry(
            &retry_prefix,
            turn,
            profile.profile_id(),
            native_session_digest(&recovery_probe),
            recovery_probe.fingerprint()?.digest(),
        )?
    };
    let planner = DeveloperRetryPlanner::new(
        turn,
        &request.cancellation,
        provider_selection,
        recovered,
    )?;
    let mut attempt = recovered.map_or(1, DeveloperRetryRecovery::next_attempt);
    let mut scheduled_attempt = recovered
        .map(DeveloperRetryRecovery::next_attempt)
        .map(|next| next.checked_sub(1).ok_or(DeveloperLoopError::LimitExceeded))
        .transpose()?;
    if let Some(recovered) = recovered {
        wait_for_recovered_retry(&request.cancellation, recovered).await?;
    }
    loop {
        if request.cancellation.is_cancelled() {
            return Err(DeveloperLoopError::Cancelled);
        }
        if let Some(current_selection) =
            provider_selection_change(interaction, profile, provider_selection)?
        {
            let _ = trace.supersede_retry_for_provider_selection(
                &retry_prefix,
                turn,
                current_selection,
            )?;
            return Ok(None);
        }
        if let Some((port, _, revision)) = interaction
            && port.input()?.revision != revision
        {
            // Context preparation or retry backoff may have admitted newer input. Return to the
            // outer context owner before building another request from the stale transcript.
            supersede_scheduled(trace, &retry_prefix, turn, &mut scheduled_attempt)?;
            return Ok(None);
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
            required_tool,
            provider.reasoning_effort(),
            remaining_tool_calls,
        )?;
        let model_request = RetryContext::bind_session(retry_context.as_ref(), kind, model_request);
        provider.validate_request(&model_request)?;
        if let Some(owner) = retry_context.as_mut() {
            owner.tools.observe_model_context(model_request.messages())?;
        }
        if !admit_role_request(
            interaction,
            provider_selection,
            profile,
            &model_request,
            turn,
            attempt,
        )? {
            if let Some(current_selection) =
                provider_selection_change(interaction, profile, provider_selection)?
            {
                let _ = trace.supersede_retry_for_provider_selection(
                    &retry_prefix,
                    turn,
                    current_selection,
                )?;
            } else {
                supersede_scheduled(trace, &retry_prefix, turn, &mut scheduled_attempt)?;
            }
            return Ok(None);
        }
        let admitted_request_id = model_request.request_id().expose_for_wire().to_owned();
        let provider_request = if let Some((port, _, _)) = interaction {
            match port.materialize_request(model_request.clone()) {
                Ok(request) => request,
                Err(error) => {
                    settle_unsent_role_request(interaction, &admitted_request_id)?;
                    return Err(error);
                }
            }
        } else {
            model_request.clone()
        };
        if let Err(error) = trace.account(DeveloperAccountingEvent::ModelRequest {
            retry: attempt > 1,
        }) {
            settle_unsent_role_request(interaction, &admitted_request_id)?;
            return Err(error);
        }
        if let Err(error) = trace.begin_retry_attempt(turn, attempt, &model_request) {
            settle_unsent_role_request(interaction, &admitted_request_id)?;
            return Err(error);
        }
        let driven = cancel_first(
            &request.cancellation,
            drive(
                provider,
                provider_request,
                protocol_limits,
                trace,
                interaction.map(|value| value.0),
            ),
        )
        .await
        .unwrap_or(Err(DeveloperLoopError::Cancelled));
        if let Some((port, role, _)) = interaction
            && request_disposition(&driven) == DeveloperRetryDisposition::Settled
        {
            let request_usage = driven.as_ref().map_or_else(
                |_| peritus_model_protocol::UsageCounters::default(),
                |outcome| outcome.session().usage_high_water(),
            );
            if port.complete_role_request(role, &admitted_request_id, request_usage)?
                == DeveloperControlFlow::Stop
            {
                return Err(DeveloperLoopError::Cancelled);
            }
        }
        match driven {
            Ok(DriveOutcome::Terminal(session))
                if successful(session.terminal()) && usable(&session) =>
            {
                usage.observe(session.usage_high_water())?;
                trace.finish_retry_attempt(
                    turn,
                    attempt,
                    &model_request,
                    DeveloperRetryDisposition::Settled,
                )?;
                return Ok(Some(session));
            }
            Ok(DriveOutcome::Terminal(session)) => {
                usage.observe(session.usage_high_water())?;
                let record = planner.terminal(
                    &model_request,
                    attempt,
                    session.terminal(),
                    usable(&session),
                )?;
                let Some(record) = record else {
                    trace.finish_retry_attempt(
                        turn,
                        attempt,
                        &model_request,
                        terminal_disposition(session.terminal()),
                    )?;
                    return Err(terminal_error(session.terminal()));
                };
                planner.record_and_wait(&record, trace).await?;
                scheduled_attempt = Some(attempt);
                *retries = retries.checked_add(1).ok_or(DeveloperLoopError::LimitExceeded)?;
            }
            Ok(DriveOutcome::Rejected { mut session, failure }) => {
                usage.observe(session.usage_high_water())?;
                let record = planner.rejection(&model_request, attempt, &failure)?;
                if let Some(record) = record {
                    planner.record(&record, trace)?;
                    settle_rejected_body(&mut session, trace).await?;
                    planner.wait(&record).await?;
                    scheduled_attempt = Some(attempt);
                    *retries =
                        retries.checked_add(1).ok_or(DeveloperLoopError::LimitExceeded)?;
                } else {
                    settle_rejected_body(&mut session, trace).await?;
                    trace.finish_retry_attempt(
                        turn,
                        attempt,
                        &model_request,
                        terminal_disposition(session.terminal()),
                    )?;
                    return Err(terminal_error(session.terminal()));
                }
            }
            Err(error) => {
                let record = planner.error(&model_request, attempt, &error)?;
                let Some(record) = record else {
                    trace.finish_retry_attempt(
                        turn,
                        attempt,
                        &model_request,
                        error_disposition(&error),
                    )?;
                    return Err(error);
                };
                planner.record_and_wait(&record, trace).await?;
                scheduled_attempt = Some(attempt);
                *retries = retries.checked_add(1).ok_or(DeveloperLoopError::LimitExceeded)?;
            }
        }
        attempt = attempt.checked_add(1).ok_or(DeveloperLoopError::LimitExceeded)?;
    }
}

fn supersede_scheduled(
    trace: &mut dyn DeveloperTrace,
    request_prefix: &str,
    turn: u16,
    scheduled_attempt: &mut Option<u64>,
) -> Result<(), DeveloperLoopError> {
    if let Some(attempt) = scheduled_attempt.take() {
        trace.supersede_retry(request_prefix, turn, attempt)?;
    }
    Ok(())
}

async fn wait_for_recovered_retry(
    cancellation: &CancellationToken,
    recovery: DeveloperRetryRecovery,
) -> Result<(), DeveloperLoopError> {
    wait_until_eligible(cancellation, recovery.next_eligible_unix_millis()).await
}

const fn request_disposition(
    result: &Result<DriveOutcome, DeveloperLoopError>,
) -> DeveloperRetryDisposition {
    match result {
        Ok(DriveOutcome::Terminal(session)) => terminal_disposition(session.terminal()),
        Ok(DriveOutcome::Rejected { failure, .. })
            if failure.certainty() == OutcomeCertainty::DefinitelyNotAccepted =>
        {
            DeveloperRetryDisposition::Settled
        }
        Ok(DriveOutcome::Rejected { .. }) => DeveloperRetryDisposition::ReconciliationRequired,
        Err(error) => error_disposition(error),
    }
}

const fn terminal_disposition(
    terminal: Option<&TerminalOutcome>,
) -> DeveloperRetryDisposition {
    match terminal {
        Some(TerminalOutcome::Failed(failure))
            if matches!(
                failure.certainty(),
                OutcomeCertainty::MaybeAccepted | OutcomeCertainty::AcceptedPartial
            ) =>
        {
            DeveloperRetryDisposition::ReconciliationRequired
        }
        Some(_) => DeveloperRetryDisposition::Settled,
        None => DeveloperRetryDisposition::ReconciliationRequired,
    }
}

const fn error_disposition(error: &DeveloperLoopError) -> DeveloperRetryDisposition {
    let DeveloperLoopError::Model(crate::ModelDriveError::Provider(error)) = error else {
        return DeveloperRetryDisposition::ReconciliationRequired;
    };
    match error.kind() {
        CoreErrorKind::InvalidEndpoint
        | CoreErrorKind::InvalidCredential
        | CoreErrorKind::InvalidRequest
        | CoreErrorKind::InvalidHttp
        | CoreErrorKind::LimitExceeded
        | CoreErrorKind::Connect
        | CoreErrorKind::InvalidRetry
        | CoreErrorKind::Configuration
        | CoreErrorKind::UnsupportedCapability
        | CoreErrorKind::Unavailable => DeveloperRetryDisposition::Settled,
        CoreErrorKind::Cancelled
        | CoreErrorKind::Transport
        | CoreErrorKind::MalformedStream => {
            DeveloperRetryDisposition::ReconciliationRequired
        }
        _ => DeveloperRetryDisposition::ReconciliationRequired,
    }
}

fn provider_selection_change(
    interaction: Option<(&dyn DeveloperInteraction, DeveloperModelRole, u64)>,
    expected_profile: &peritus_model_protocol::ProviderProfile,
    expected_selection: Option<peritus_types::Sha256Digest>,
) -> Result<Option<peritus_types::Sha256Digest>, DeveloperLoopError> {
    let (Some((port, role, _)), Some(expected_selection)) = (interaction, expected_selection) else {
        return Ok(None);
    };
    let current = port.provider_selection(role)?.ok_or_else(|| {
        DeveloperLoopError::RecoveryRequired(
            "the host no longer resolves the explicitly selected provider".to_owned(),
        )
    })?;
    let current_selection = current.provenance().ok_or_else(|| {
        DeveloperLoopError::RecoveryRequired(
            "the host cannot prove current provider-selection provenance".to_owned(),
        )
    })?;
    if current_selection != expected_selection {
        return Ok(Some(current_selection));
    }
    if current.provider().profile().profile_id() != expected_profile.profile_id() {
        return Err(DeveloperLoopError::RecoveryRequired(
            "the resolved provider profile changed without an explicit selection change"
                .to_owned(),
        ));
    }
    Ok(None)
}

fn admit_role_request(
    interaction: Option<(&dyn DeveloperInteraction, DeveloperModelRole, u64)>,
    provider_selection: Option<peritus_types::Sha256Digest>,
    profile: &peritus_model_protocol::ProviderProfile,
    request: &ModelRequest,
    turn: u16,
    attempt: u64,
) -> Result<bool, DeveloperLoopError> {
    let Some((port, role, revision)) = interaction else { return Ok(true) };
    match port.prepare_selected_role_request(role, revision, provider_selection, request)? {
        DeveloperRequestAdmission::Accepted => {}
        DeveloperRequestAdmission::Stale => return Ok(false),
        DeveloperRequestAdmission::Stopped => return Err(DeveloperLoopError::Cancelled),
    }
    let request_identity = DeveloperProviderRequestIdentity::from_parts(
        role,
        turn,
        attempt,
        peritus_codec::sha256(request.request_id().expose_for_wire().as_bytes()),
        request.fingerprint()?.digest(),
        request.profile_id(),
        request.profile_revision(),
        peritus_codec::sha256(request.provider().as_str().as_bytes()),
        native_session_digest(request),
        provider_selection,
    );
    port.observe(DeveloperActivity::ModelStarted {
        model: profile.model().as_str(),
        reasoning: request.options().reasoning(),
        request: request_identity,
    })?;
    Ok(true)
}

fn settle_unsent_role_request(
    interaction: Option<(&dyn DeveloperInteraction, DeveloperModelRole, u64)>,
    request_id: &str,
) -> Result<(), DeveloperLoopError> {
    if let Some((port, role, _)) = interaction {
        let _ = port.complete_role_request(
            role,
            request_id,
            peritus_model_protocol::UsageCounters::default(),
        )?;
    }
    Ok(())
}

async fn drive(
    provider: &dyn ModelProvider,
    model_request: ModelRequest,
    protocol_limits: ProtocolLimits,
    trace: &mut dyn DeveloperTrace,
    interaction: Option<&dyn DeveloperInteraction>,
) -> Result<DriveOutcome, DeveloperLoopError> {
    // OwnedModelStream cancels its token when a stream fails or is dropped. That cleanup must
    // stop only this attempt, leaving the caller's token active for safe automatic reconnects.
    let attempt = AttemptCancellation(CancellationToken::new());
    let mut progress = progress::ProviderProgress::new(interaction);
    let mut session = progress
        .wait(async {
            ModelSession::start(provider, model_request, protocol_limits, attempt.0.clone())
                .await
                .map_err(DeveloperLoopError::from)
        })
        .await?;
    let result = async {
        loop {
            match progress
                .wait(async { session.pull_one().await.map_err(DeveloperLoopError::from) })
                .await?
            {
                ModelAdvance::Closed => {
                    return Ok::<Option<ModelFailure>, DeveloperLoopError>(None);
                }
                ModelAdvance::EnvelopePending { .. } => {
                    let encoded = session.encode_pending()?;
                    trace.record(DeveloperTraceEvent::ProviderEnvelope(&encoded))?;
                    let public_text =
                        session.pending().and_then(|envelope| match envelope.event() {
                            ModelEvent::TextDelta { fragment, .. } => {
                                Some((false, fragment.expose().to_vec()))
                            }
                            ModelEvent::ReasoningSummaryDelta { fragment, .. } => {
                                Some((true, fragment.expose().to_vec()))
                            }
                            _ => None,
                        });
                    let has_usage = session
                        .pending()
                        .is_some_and(|envelope| matches!(envelope.event(), ModelEvent::Usage(_)));
                    let healed = session.pending().is_some_and(|envelope| {
                        matches!(
                            envelope.event(), ModelEvent::ProviderEvent(extension)
                            if extension.name().as_str() == "peritus.response_healing"
                        )
                    });
                    let rejection = session.pending().and_then(|envelope| {
                        if let ModelEvent::ResponseRejected(failure) = envelope.event() {
                            Some(failure.clone())
                        } else {
                            None
                        }
                    });
                    let _ = session.accept_durable_pending()?;
                    if healed && let Some(port) = interaction {
                        port.observe(DeveloperActivity::ResponseHealed)?;
                    }
                    if has_usage {
                        // Cancellation/deadline can drop this future before a terminal response.
                        trace
                            .account(DeveloperAccountingEvent::Usage(session.usage_high_water()))?;
                    }
                    if let (Some(port), Some((summary, text))) = (interaction, public_text) {
                        // Never insert waiting messages between fragments of public assistant text.
                        if summary {
                            progress.summary_received();
                            port.observe(DeveloperActivity::ReasoningSummary(&text))?;
                        } else {
                            progress.text_received();
                            port.observe(DeveloperActivity::Text(&text))?;
                        }
                    }
                    if rejection.is_some() {
                        return Ok(rejection);
                    }
                }
            }
        }
    }
    .await;
    // A later stream/trace/tool failure must not erase usage already accepted by the reducer.
    trace.account(DeveloperAccountingEvent::Usage(session.usage_high_water()))?;
    let rejection = result?;
    Ok(match rejection {
        Some(failure) => DriveOutcome::Rejected { session, failure },
        None => DriveOutcome::Terminal(session),
    })
}

async fn settle_rejected_body(
    session: &mut ModelSession,
    trace: &mut dyn DeveloperTrace,
) -> Result<(), DeveloperLoopError> {
    session.cancel();
    let ModelAdvance::EnvelopePending { .. } = session.pull_one().await? else {
        return Err(crate::ModelDriveError::InvalidContinuation.into());
    };
    if !session
        .pending()
        .is_some_and(|envelope| matches!(envelope.event(), ModelEvent::ResponseFailed(_)))
    {
        return Err(crate::ModelDriveError::InvalidContinuation.into());
    }
    let encoded = session.encode_pending()?;
    trace.record(DeveloperTraceEvent::ProviderEnvelope(&encoded))?;
    let _ = session.accept_durable_pending()?;
    if session.terminal().is_none() {
        return Err(crate::ModelDriveError::InvalidContinuation.into());
    }
    Ok(())
}

struct AttemptCancellation(CancellationToken);

impl Drop for AttemptCancellation {
    fn drop(&mut self) {
        // This also covers cancellation while provider.start is still pending, before an owned
        // stream exists, and dropping the entire developer loop after explicit cancellation.
        let _ = self.0.cancel();
    }
}
