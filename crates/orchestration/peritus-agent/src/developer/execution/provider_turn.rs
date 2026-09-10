//! Bounded provider retries and durable public-text delivery.
use super::super::{
    DeveloperAccountingEvent, DeveloperActivity, DeveloperControlFlow, DeveloperInteraction,
    DeveloperLoopError, DeveloperLoopRequest, DeveloperModelRole, DeveloperRequestAdmission,
    DeveloperTrace, DeveloperTraceEvent, DeveloperUsage,
    model_request::{ModelTurnKind, build_model_request},
    retry::DeveloperRetryPlanner,
};
use super::{successful, terminal_error, usable};
use crate::{ModelAdvance, ModelSession};
use peritus_model_protocol::{Message, ModelEvent, ModelRequest, ProtocolLimits};
use peritus_provider_core::ModelProvider;

mod progress;

#[allow(clippy::too_many_arguments, reason = "one logical turn keeps its checked request inputs")]
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
    retries: &mut u16,
    usage: &mut DeveloperUsage,
    trace: &mut dyn DeveloperTrace,
    interaction: Option<(&dyn DeveloperInteraction, DeveloperModelRole, u64)>,
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
        if let Some((port, role, revision)) = interaction {
            match port.prepare_role_request(role, revision, &model_request)? {
                DeveloperRequestAdmission::Accepted => {}
                DeveloperRequestAdmission::Stale => return Ok(None),
                DeveloperRequestAdmission::Stopped => {
                    return Err(DeveloperLoopError::Cancelled);
                }
            }
            port.observe(DeveloperActivity::ModelStarted {
                model: profile.model().as_str(),
                reasoning: model_request.options().reasoning(),
            })?;
        }
        let admitted_request_id = model_request.request_id().expose_for_wire().to_owned();
        trace.account(DeveloperAccountingEvent::ModelRequest { retry: attempt > 1 })?;
        let driven = drive(
            provider,
            model_request,
            request,
            protocol_limits,
            trace,
            interaction.map(|value| value.0),
        )
        .await;
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

async fn drive(
    provider: &dyn ModelProvider,
    model_request: ModelRequest,
    request: &DeveloperLoopRequest,
    protocol_limits: ProtocolLimits,
    trace: &mut dyn DeveloperTrace,
    interaction: Option<&dyn DeveloperInteraction>,
) -> Result<ModelSession, DeveloperLoopError> {
    let mut progress = progress::ProviderProgress::new(interaction, &request.cancellation);
    let mut session = progress
        .wait(async {
            ModelSession::start(
                provider,
                model_request,
                protocol_limits,
                request.cancellation.clone(),
            )
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
                                Some(fragment.expose().to_vec())
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
                    if let (Some(port), Some(text)) = (interaction, public_text) {
                        // Never insert waiting messages between fragments of public assistant text.
                        progress.text_received();
                        port.observe(DeveloperActivity::Text(&text))?;
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
