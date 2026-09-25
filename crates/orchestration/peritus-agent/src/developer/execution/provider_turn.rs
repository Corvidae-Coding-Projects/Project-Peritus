//! Bounded provider retries and durable public-text delivery.
use super::super::{
    DeveloperAccountingEvent, DeveloperActivity, DeveloperControlFlow, DeveloperInteraction,
    DeveloperLoopError, DeveloperLoopRequest, DeveloperModelRole, DeveloperRequestAdmission,
    DeveloperToolExecutor, DeveloperTrace, DeveloperTraceEvent, DeveloperUsage,
    model_request::{ModelTurnKind, build_model_request},
    retry::DeveloperRetryPlanner,
};
use super::{ContextSession, prepare_messages, successful, terminal_error, usable};
use crate::{ModelAdvance, ModelSession};
use peritus_model_protocol::{Message, ModelEvent, ModelRequest, ProtocolLimits};
use peritus_provider_core::{CancellationToken, ModelProvider, cancel_first};

mod progress;

pub(super) struct RetryContext<'a, 'port> {
    pub(super) context: &'a mut ContextSession<'port>,
    pub(super) tools: &'a mut dyn DeveloperToolExecutor,
    pub(super) governing_input: Option<&'a Message>,
    pub(super) compactions: &'a mut u16,
}

impl RetryContext<'_, '_> {
    fn prepare(
        owner: Option<&mut Self>,
        request: &DeveloperLoopRequest,
        messages: &[Message],
        profile: &peritus_model_protocol::ProviderProfile,
        position: (u16, ModelTurnKind, u8, Option<&str>),
        trace: &mut dyn DeveloperTrace,
    ) -> Result<Option<Vec<Message>>, DeveloperLoopError> {
        let (step, kind, attempt, required_tool) = position;
        if kind != ModelTurnKind::Developer || attempt == 1 {
            return Ok(None);
        }
        let Some(required_tool) = required_tool else { return Ok(None) };
        let owner = owner.ok_or_else(|| {
            DeveloperLoopError::Context("required-tool retry has no context owner".to_owned())
        })?;
        owner.assemble(request, messages, profile, (step, attempt, required_tool), trace).map(Some)
    }

    fn assemble(
        &mut self,
        request: &DeveloperLoopRequest,
        messages: &[Message],
        profile: &peritus_model_protocol::ProviderProfile,
        position: (u16, u8, &str),
        trace: &mut dyn DeveloperTrace,
    ) -> Result<Vec<Message>, DeveloperLoopError> {
        let (step, attempt, required_tool) = position;
        let policy = super::invocation::retry_policy(request, step, attempt, required_tool)?;
        let mut messages = messages.to_vec();
        messages[0] = policy.clone();
        let count = if self.context.is_local() {
            u16::from(self.context.prepare(
                &mut messages,
                &request.tools,
                profile,
                &policy,
                self.governing_input,
            )?)
        } else {
            let records = prepare_messages(
                &mut messages,
                &request.tools,
                profile,
                request.limits.max_output_tokens(),
                ProtocolLimits::PRODUCTION,
                if self.governing_input.is_some() { 3 } else { 2 },
            )?;
            for record in &records {
                trace.record(DeveloperTraceEvent::ContextCompaction(record))?;
            }
            u16::try_from(records.len()).map_err(|_| DeveloperLoopError::LimitExceeded)?
        };
        for _ in 0..count {
            trace.account(DeveloperAccountingEvent::Compaction)?;
        }
        *self.compactions =
            self.compactions.checked_add(count).ok_or(DeveloperLoopError::LimitExceeded)?;
        Ok(messages)
    }
}

#[allow(clippy::too_many_arguments, reason = "one logical turn keeps its checked request inputs")]
pub(super) async fn complete_turn(
    provider: &dyn ModelProvider,
    request: &DeveloperLoopRequest,
    messages: &mut Vec<Message>,
    profile: &peritus_model_protocol::ProviderProfile,
    negotiated: peritus_model_protocol::NegotiatedCapabilities,
    protocol_limits: ProtocolLimits,
    turn: u16,
    kind: ModelTurnKind,
    required_tool: Option<&str>,
    retries: &mut u16,
    usage: &mut DeveloperUsage,
    trace: &mut dyn DeveloperTrace,
    interaction: Option<(&dyn DeveloperInteraction, DeveloperModelRole, u64)>,
    mut retry_context: Option<RetryContext<'_, '_>>,
) -> Result<Option<ModelSession>, DeveloperLoopError> {
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
        if let Some((port, _, revision)) = interaction
            && port.input()?.revision != revision
        {
            // Context preparation or retry backoff may have admitted newer input. Return to the
            // outer context owner before building another request from the stale transcript.
            return Ok(None);
        }
        if let Some(prepared) = RetryContext::prepare(
            retry_context.as_mut(),
            request,
            messages,
            profile,
            (turn, kind, attempt, required_tool),
            trace,
        )? {
            *messages = prepared;
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
        )?;
        if let Some(owner) = retry_context.as_mut() {
            owner.tools.observe_model_context(model_request.messages())?;
        }
        if !admit_role_request(interaction, profile, &model_request)? {
            return Ok(None);
        }
        let admitted_request_id = model_request.request_id().expose_for_wire().to_owned();
        trace.account(DeveloperAccountingEvent::ModelRequest { retry: attempt > 1 })?;
        let driven = cancel_first(
            &request.cancellation,
            drive(
                provider,
                model_request,
                protocol_limits,
                trace,
                interaction.map(|value| value.0),
            ),
        )
        .await
        .unwrap_or(Err(DeveloperLoopError::Cancelled));
        if let Some((port, role, _)) = interaction {
            let request_usage = driven.as_ref().map_or_else(
                |_| peritus_model_protocol::UsageCounters::default(),
                ModelSession::usage_high_water,
            );
            if port.complete_role_request(role, &admitted_request_id, request_usage)?
                == DeveloperControlFlow::Stop
            {
                return Err(DeveloperLoopError::Cancelled);
            }
        }
        match driven {
            Ok(session) if successful(session.terminal()) && usable(&session) => {
                usage.observe(session.usage_high_water())?;
                return Ok(Some(session));
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

fn admit_role_request(
    interaction: Option<(&dyn DeveloperInteraction, DeveloperModelRole, u64)>,
    profile: &peritus_model_protocol::ProviderProfile,
    request: &ModelRequest,
) -> Result<bool, DeveloperLoopError> {
    let Some((port, role, revision)) = interaction else { return Ok(true) };
    match port.prepare_role_request(role, revision, request)? {
        DeveloperRequestAdmission::Accepted => {}
        DeveloperRequestAdmission::Stale => return Ok(false),
        DeveloperRequestAdmission::Stopped => return Err(DeveloperLoopError::Cancelled),
    }
    port.observe(DeveloperActivity::ModelStarted {
        model: profile.model().as_str(),
        reasoning: request.options().reasoning(),
    })?;
    Ok(true)
}

async fn drive(
    provider: &dyn ModelProvider,
    model_request: ModelRequest,
    protocol_limits: ProtocolLimits,
    trace: &mut dyn DeveloperTrace,
    interaction: Option<&dyn DeveloperInteraction>,
) -> Result<ModelSession, DeveloperLoopError> {
    // OwnedModelStream cancels its token when a stream fails or is dropped. That cleanup must
    // stop only this attempt, leaving the caller's token active for bounded automatic retries.
    let attempt = AttemptCancellation(CancellationToken::new());
    let mut progress = progress::ProviderProgress::new(interaction, &attempt.0);
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
                ModelAdvance::Closed => return Ok::<(), DeveloperLoopError>(()),
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
                }
            }
        }
    }
    .await;
    // A later stream/trace/tool failure must not erase usage already accepted by the reducer.
    trace.account(DeveloperAccountingEvent::Usage(session.usage_high_water()))?;
    result?;
    Ok(session)
}

struct AttemptCancellation(CancellationToken);

impl Drop for AttemptCancellation {
    fn drop(&mut self) {
        // This also covers cancellation while provider.start is still pending, before an owned
        // stream exists, and dropping the entire developer loop at its deadline.
        let _ = self.0.cancel();
    }
}
