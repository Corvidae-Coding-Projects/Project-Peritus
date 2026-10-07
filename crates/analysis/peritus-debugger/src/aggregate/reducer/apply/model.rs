//! Optional model-attempt transitions kept separate from deterministic job phases.

use peritus_types::Sha256Digest;

use crate::{
    DebuggerError, DebuggerErrorKind, DebuggerOperation, DebuggerRecovery, ModelAnalysisId,
};

use super::super::super::{
    DebuggerCommand, DebuggerEventKind, DebuggerPhase, DebuggerState, ModelAttemptFailure,
    ModelAttemptFailureCode, ModelAttemptObservation, ModelAttemptResult, ModelBudget,
    ModelProgress, ModelRetryPolicy, ModelWorkState,
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
    if model.id() != model_id || *expected != attempt || started_at_tick < *not_before_tick {
        return Err(conflict("model attempt identity, sequence, or schedule differs"));
    }
    let mut state = prior.clone();
    state.model =
        Some(model.clone().with_state(ModelWorkState::Running { attempt, started_at_tick }));
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
    let cumulative = prior.model_attempts().iter().any(|observation| {
        matches!(
            observation.result(),
            ModelAttemptResult::Failure(failure) if !failure.is_legacy()
        )
    });
    let accounting = accumulated_accounting(prior).with_attempt(
        event_count,
        output_bytes,
        input_tokens,
        output_tokens,
        total_tokens,
    );
    if if cumulative {
        accounting.exceeds(budget)
    } else {
        output_bytes > budget.max_output_bytes()
            || event_count > budget.max_events()
            || input_tokens > budget.max_input_tokens()
            || output_tokens > budget.max_output_tokens()
            || total_tokens > budget.max_total_tokens()
    }
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
    state.model = Some(
        model.clone().with_state(ModelWorkState::Validated { attempt, proposal_digest }),
    );
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
    if failure.context().is_some_and(|context| {
        context.profile_id() != prior.revision().provider_profile_id()
    }) {
        return Err(conflict("model failure belongs to another provider profile"));
    }
    if failure.is_legacy()
        && (failure.event_count() > model.budget().max_events()
            || failure.total_tokens() > model.budget().max_total_tokens())
    {
        return Err(budget_error("model failure accounting exceeds the frozen budget"));
    }
    let accounting = accumulated_accounting(prior).with_attempt(
        failure.event_count(),
        failure.output_bytes(),
        failure.input_tokens(),
        failure.output_tokens(),
        failure.total_tokens(),
    );
    let observation = ModelAttemptObservation::new(
        failure.model_id(),
        failure.attempt(),
        ModelAttemptResult::Failure(failure.clone()),
    )?;
    let next_attempt = failure.attempt().checked_add(1);
    let cancelled = failure.code() == ModelAttemptFailureCode::Cancelled;
    let can_retry = !cancelled
        && (failure.is_legacy() || !accounting.exceeds(model.budget()))
        && failure.retryable()
        && next_attempt.is_some_and(|attempt| model.retry_policy().permits(attempt));
    let model_state = if can_retry {
        ModelWorkState::AwaitingRetry { attempt: failure.attempt(), failure: failure.clone() }
    } else {
        ModelWorkState::Rejected { attempt: failure.attempt(), failure: failure.clone() }
    };
    let mut state = prior.clone();
    state.model_attempts.push(observation);
    state.model = Some(model.clone().with_state(model_state));
    state.phase = if cancelled {
        state.cancellation_reason_digest = Some(failure.diagnostic_digest());
        DebuggerPhase::Cancelled
    } else if can_retry {
        DebuggerPhase::ModelPending
    } else {
        DebuggerPhase::DeterministicComplete
    };
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
        || !model.retry_policy().permits(next_attempt)
    {
        return Err(conflict("retry identity or attempt is inconsistent"));
    }
    let mut state = prior.clone();
    state.model = Some(model.clone().with_state(ModelWorkState::Pending {
        attempt: next_attempt,
        not_before_tick,
    }));
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
            } if *current == attempt
        )
    {
        return Err(conflict("model settlement differs from the running attempt"));
    }
    Ok(model.clone())
}

#[derive(Clone, Copy, Default)]
struct AttemptAccounting {
    events: u64,
    output_bytes: u64,
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
}

impl AttemptAccounting {
    fn with_attempt(
        self,
        events: u64,
        output_bytes: u64,
        input_tokens: u64,
        output_tokens: u64,
        total_tokens: u64,
    ) -> Self {
        Self {
            events: self.events.saturating_add(events),
            output_bytes: self.output_bytes.saturating_add(output_bytes),
            input_tokens: self.input_tokens.saturating_add(input_tokens),
            output_tokens: self.output_tokens.saturating_add(output_tokens),
            total_tokens: self.total_tokens.saturating_add(total_tokens),
        }
    }

    const fn exceeds(self, budget: ModelBudget) -> bool {
        self.events > budget.max_events()
            || self.output_bytes > budget.max_output_bytes()
            || self.input_tokens > budget.max_input_tokens()
            || self.output_tokens > budget.max_output_tokens()
            || self.total_tokens > budget.max_total_tokens()
    }
}

fn accumulated_accounting(state: &DebuggerState) -> AttemptAccounting {
    let mut accounting = AttemptAccounting::default();
    for observation in state.model_attempts() {
        accounting = match observation.result() {
            ModelAttemptResult::Proposal {
                output_bytes,
                event_count,
                input_tokens,
                output_tokens,
                total_tokens,
                ..
            } => accounting.with_attempt(
                *event_count,
                *output_bytes,
                *input_tokens,
                *output_tokens,
                *total_tokens,
            ),
            ModelAttemptResult::Failure(failure) => accounting.with_attempt(
                failure.event_count(),
                failure.output_bytes(),
                failure.input_tokens(),
                failure.output_tokens(),
                failure.total_tokens(),
            ),
        };
    }
    accounting
}

fn budget_error(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Budget,
        DebuggerOperation::ApplyTransition,
        DebuggerRecovery::CorrectInput,
        detail,
    )
}
