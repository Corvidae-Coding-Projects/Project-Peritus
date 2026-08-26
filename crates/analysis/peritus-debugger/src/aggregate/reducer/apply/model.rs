//! Optional model-attempt transitions kept separate from deterministic job phases.

use peritus_types::Sha256Digest;

use crate::{
    DebuggerError, DebuggerErrorKind, DebuggerOperation, DebuggerRecovery, ModelAnalysisId,
};

use super::super::super::{
    DebuggerCommand, DebuggerEventKind, DebuggerPhase, DebuggerState, ModelAttemptFailure,
    ModelAttemptObservation, ModelAttemptResult, ModelBudget, ModelProgress, ModelRetryPolicy,
    ModelWorkState,
};
use super::{advance, conflict, illegal, require_phase};

#[allow(clippy::too_many_arguments, reason = "frozen model request bindings remain explicit")]
pub(super) fn request(
    prior: &DebuggerState,
    command: &DebuggerCommand,
    sequence: u64,
    model_id: ModelAnalysisId,
    plan_digest: Sha256Digest,
    request_digest: Sha256Digest,
    budget: ModelBudget,
    retry_policy: ModelRetryPolicy,
) -> Result<(DebuggerEventKind, DebuggerState), DebuggerError> {
    require_phase(prior, DebuggerPhase::DeterministicComplete)?;
    if prior.model().is_some() || prior.model_plan_digest() != Some(plan_digest) {
        return Err(conflict("model request differs from frozen job plan"));
    }
    let mut state = prior.clone();
    state.model =
        Some(ModelProgress::new(model_id, plan_digest, request_digest, budget, retry_policy));
    state.phase = DebuggerPhase::ModelPending;
    advance(&mut state, command, sequence);
    Ok((
        DebuggerEventKind::ModelAnalysisRequested {
            model_id,
            plan_digest,
            request_digest,
            budget,
            retry_policy,
        },
        state,
    ))
}

pub(super) fn start(
    prior: &DebuggerState,
    command: &DebuggerCommand,
    sequence: u64,
    model_id: ModelAnalysisId,
    attempt: u16,
    started_at_tick: u64,
) -> Result<(DebuggerEventKind, DebuggerState), DebuggerError> {
    require_phase(prior, DebuggerPhase::ModelPending)?;
    let model = prior.model().ok_or_else(|| illegal("model-pending state has no model plan"))?;
    let ModelWorkState::Pending { attempt: expected, not_before_tick } = model.state() else {
        return Err(illegal("model attempt cannot start before retry scheduling"));
    };
    if model.id() != model_id || expected != attempt || started_at_tick < not_before_tick {
        return Err(conflict("model attempt identity, sequence, or schedule differs"));
    }
    let mut state = prior.clone();
    state.model = Some(model.with_state(ModelWorkState::Running { attempt, started_at_tick }));
    state.phase = DebuggerPhase::ModelRunning;
    advance(&mut state, command, sequence);
    Ok((DebuggerEventKind::ModelAttemptStarted { model_id, attempt, started_at_tick }, state))
}

#[allow(clippy::too_many_arguments, reason = "model settlement accounting remains explicit")]
pub(super) fn record_proposal(
    prior: &DebuggerState,
    command: &DebuggerCommand,
    sequence: u64,
    model_id: ModelAnalysisId,
    attempt: u16,
    proposal_digest: Sha256Digest,
    output_digest: Sha256Digest,
    output_bytes: u64,
    event_count: u64,
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
) -> Result<(DebuggerEventKind, DebuggerState), DebuggerError> {
    require_phase(prior, DebuggerPhase::ModelRunning)?;
    let model = running_model(prior, model_id, attempt)?;
    let budget = model.budget();
    if output_bytes > budget.max_output_bytes()
        || event_count > budget.max_events()
        || input_tokens > budget.max_input_tokens()
        || output_tokens > budget.max_output_tokens()
        || total_tokens > budget.max_total_tokens()
    {
        return Err(budget_error("validated model result exceeds the frozen job budget"));
    }
    let observation = ModelAttemptObservation::new(
        model_id,
        attempt,
        ModelAttemptResult::Proposal {
            proposal_digest,
            output_digest,
            output_bytes,
            event_count,
            input_tokens,
            output_tokens,
            total_tokens,
        },
    )?;
    let mut state = prior.clone();
    state.model_attempts.push(observation);
    state.model = Some(model.with_state(ModelWorkState::Validated { attempt, proposal_digest }));
    state.phase = DebuggerPhase::ModelValidated;
    advance(&mut state, command, sequence);
    Ok((
        DebuggerEventKind::ModelProposalRecorded {
            model_id,
            attempt,
            proposal_digest,
            output_digest,
            output_bytes,
            event_count,
            input_tokens,
            output_tokens,
            total_tokens,
        },
        state,
    ))
}

pub(super) fn record_failure(
    prior: &DebuggerState,
    command: &DebuggerCommand,
    sequence: u64,
    failure: ModelAttemptFailure,
) -> Result<(DebuggerEventKind, DebuggerState), DebuggerError> {
    require_phase(prior, DebuggerPhase::ModelRunning)?;
    let model = running_model(prior, failure.model_id(), failure.attempt())?;
    if failure.event_count() > model.budget().max_events()
        || failure.total_tokens() > model.budget().max_total_tokens()
    {
        return Err(budget_error("model failure accounting exceeds the frozen budget"));
    }
    let observation = ModelAttemptObservation::new(
        failure.model_id(),
        failure.attempt(),
        ModelAttemptResult::Failure(failure),
    )?;
    let can_retry = failure.retryable() && failure.attempt() < model.retry_policy().max_attempts();
    let model_state = if can_retry {
        ModelWorkState::AwaitingRetry { attempt: failure.attempt(), failure }
    } else {
        ModelWorkState::Rejected { attempt: failure.attempt(), failure }
    };
    let mut state = prior.clone();
    state.model_attempts.push(observation);
    state.model = Some(model.with_state(model_state));
    state.phase =
        if can_retry { DebuggerPhase::ModelPending } else { DebuggerPhase::DeterministicComplete };
    advance(&mut state, command, sequence);
    Ok((DebuggerEventKind::ModelFailureRecorded { failure }, state))
}

pub(super) fn schedule_retry(
    prior: &DebuggerState,
    command: &DebuggerCommand,
    sequence: u64,
    model_id: ModelAnalysisId,
    next_attempt: u16,
    not_before_tick: u64,
) -> Result<(DebuggerEventKind, DebuggerState), DebuggerError> {
    require_phase(prior, DebuggerPhase::ModelPending)?;
    let model = prior.model().ok_or_else(|| illegal("retry scheduling has no model plan"))?;
    let ModelWorkState::AwaitingRetry { attempt, .. } = model.state() else {
        return Err(illegal("retry scheduling requires a retryable settled failure"));
    };
    if model.id() != model_id
        || attempt.checked_add(1) != Some(next_attempt)
        || next_attempt > model.retry_policy().max_attempts()
    {
        return Err(conflict("retry identity or attempt is inconsistent"));
    }
    let mut state = prior.clone();
    state.model =
        Some(model.with_state(ModelWorkState::Pending { attempt: next_attempt, not_before_tick }));
    advance(&mut state, command, sequence);
    Ok((DebuggerEventKind::ModelRetryScheduled { model_id, next_attempt, not_before_tick }, state))
}

fn running_model(
    state: &DebuggerState,
    model_id: ModelAnalysisId,
    attempt: u16,
) -> Result<ModelProgress, DebuggerError> {
    let model = state.model().ok_or_else(|| illegal("model-running state has no model plan"))?;
    if model.id() != model_id
        || !matches!(
            model.state(),
            ModelWorkState::Running {
                attempt: current,
                ..
            } if current == attempt
        )
    {
        return Err(conflict("model settlement differs from the running attempt"));
    }
    Ok(model)
}

fn budget_error(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Budget,
        DebuggerOperation::ApplyTransition,
        DebuggerRecovery::CorrectInput,
        detail,
    )
}
