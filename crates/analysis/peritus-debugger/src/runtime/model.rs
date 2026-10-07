//! Commit-before-provider model attempt execution and caller-controlled retry scheduling.

use peritus_journal::SqliteJournal;
use peritus_policy::AuthorityInstant;
use peritus_provider_core::{CancellationToken, ModelProvider};

use crate::{
    DebuggerCommand, DebuggerCommandKind, DebuggerError, DebuggerErrorKind, DebuggerOperation,
    DebuggerPhase, DebuggerRecovery, DebuggerState, ModelAnalysisPlan, ModelAttemptFailure,
    ModelAttemptFailureCode, ModelAttemptResult, ModelDirective, ModelDirectiveClaim, ModelPriorUsage,
    ModelRunFailure, ModelRunSuccess, ModelWorkState, TraceSelectionManifest,
    ModelRetryPolicy, ModelRetrySchedule, ModelStartBasis, ValidatedModelProposal,
    commit_debugger_claimed_transition, commit_debugger_settlement, commit_debugger_transition,
    decide, run_model_analysis_with_usage,
};

use super::{CommittedDebuggerTransition, TransitionIds};

/// Caller-reserved identities for attempt-start and exact settlement transitions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelAttemptIds {
    start: TransitionIds,
    settlement: TransitionIds,
}

impl ModelAttemptIds {
    /// Creates a pair of stable command/event identity reservations.
    #[must_use]
    pub const fn new(start: TransitionIds, settlement: TransitionIds) -> Self {
        Self { start, settlement }
    }
}

/// Durable semantic result of one optional model attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelAttemptOutcome {
    /// Exactly one strict proposal passed E2 validation.
    Proposal(ValidatedModelProposal),
    /// Provider/protocol/validation/budget failure was retained with recovery evidence.
    Failed(ModelAttemptFailure),
    /// Cooperative cancellation won and the job became terminal.
    Cancelled,
}

/// Both C0 transition observations around one provider call.
#[derive(Debug)]
pub struct ModelAttemptExecution {
    started: CommittedDebuggerTransition,
    settled: CommittedDebuggerTransition,
    outcome: ModelAttemptOutcome,
}

/// Settlement produced after reclaiming an attempt whose durable start already exists.
#[derive(Debug)]
pub struct ResumedModelAttemptExecution {
    settled: CommittedDebuggerTransition,
    outcome: ModelAttemptOutcome,
}

impl ResumedModelAttemptExecution {
    /// Atomic result/failure/cancellation plus acknowledgement under the reclaimed fence.
    #[must_use]
    pub const fn settled(&self) -> &CommittedDebuggerTransition {
        &self.settled
    }
    /// Durable semantic outcome for the same already-started attempt identity.
    #[must_use]
    pub const fn outcome(&self) -> &ModelAttemptOutcome {
        &self.outcome
    }
    /// Consumes the complete reclaimed-attempt result.
    #[must_use]
    pub fn into_parts(self) -> (CommittedDebuggerTransition, ModelAttemptOutcome) {
        (self.settled, self.outcome)
    }
}

impl ModelAttemptExecution {
    /// Commit proving attempt intent preceded provider I/O.
    #[must_use]
    pub const fn started(&self) -> &CommittedDebuggerTransition {
        &self.started
    }
    /// Atomic result/failure/cancellation plus outbox acknowledgement commit.
    #[must_use]
    pub const fn settled(&self) -> &CommittedDebuggerTransition {
        &self.settled
    }
    /// Durable semantic attempt outcome.
    #[must_use]
    pub const fn outcome(&self) -> &ModelAttemptOutcome {
        &self.outcome
    }
}

/// Claims, commits intent, runs C5, and atomically settles the exact directive.
///
/// # Errors
/// Rejects plan/claim/state drift, stale C0 fences, provider/protocol failures that cannot be
/// durably represented, or settlement failure. Ordinary provider failures are returned as a
/// successful `ModelAttemptExecution::Failed` because their typed result is durable.
#[allow(clippy::too_many_arguments, reason = "effect owners and fences remain explicit")]
pub async fn execute_model_attempt(
    journal: &mut SqliteJournal,
    provider: &dyn ModelProvider,
    state: &DebuggerState,
    plan: &ModelAnalysisPlan,
    manifest: &TraceSelectionManifest,
    debugger_limits: crate::DebuggerLimits,
    claim: ModelDirectiveClaim,
    started_at: AuthorityInstant,
    ids: ModelAttemptIds,
    cancellation: CancellationToken,
) -> Result<ModelAttemptExecution, DebuggerError> {
    let directive = claim.directive();
    let model =
        state.model().ok_or_else(|| binding("model directive has no durable model plan"))?;
    if state.phase() != DebuggerPhase::ModelPending
        || directive.job_id() != state.job_id()
        || directive.model_id() != plan.id()
        || directive.plan_digest() != plan.digest()
        || directive.request_digest() != plan.request_digest()
        || model.id() != plan.id()
        || model.plan_digest() != plan.digest()
        || model.request_digest() != plan.request_digest()
        || model.budget() != plan.budget()
        || !match model.state() {
            ModelWorkState::PendingOnClock { attempt, schedule } => {
                *attempt == directive.attempt() && directive.schedule() == Some(*schedule)
            }
            ModelWorkState::Pending { attempt, not_before_tick } => {
                *attempt == directive.attempt()
                    && directive.schedule().is_none()
                    && *not_before_tick == directive.not_before_tick()
            }
            _ => false,
        }
    {
        return Err(binding("model state, plan, and claimed directive differ"));
    }
    let basis = if let Some(schedule) = directive.schedule() {
        schedule
            .admission(started_at)
            .ok_or_else(|| binding("claimed model retry is not yet authority-clock eligible"))?
    } else if directive.attempt() == 1 && directive.not_before_tick() == 0 {
        ModelStartBasis::Immediate
    } else if directive.attempt() > 1
        && started_at.tick_millis() >= directive.not_before_tick()
    {
        ModelStartBasis::LegacyTick
    } else {
        return Err(binding("claimed legacy model retry is not yet eligible"));
    };
    let start_command = command(
        state,
        ids.start,
        DebuggerCommandKind::MarkModelAttemptStartedOnClock {
            model_id: plan.id(),
            attempt: directive.attempt(),
            started_at,
            basis,
        },
    )?;
    let start_transition = decide(Some(state), &start_command)?;
    let start_batch =
        commit_debugger_claimed_transition(journal, &start_command, &start_transition, claim)?;
    let running = start_transition.state().clone();
    let started = CommittedDebuggerTransition::new(start_batch, running.clone());
    let (settled, outcome) = settle_model_attempt_effect(
        journal,
        provider,
        &running,
        plan,
        manifest,
        debugger_limits,
        claim,
        ids.settlement,
        cancellation,
    )
    .await?;
    Ok(ModelAttemptExecution {
        started,
        settled,
        outcome,
    })
}

/// Re-executes and settles the same attempt after its committed start outlived a claim lease.
///
/// The caller must first reclaim the retained persistent directive and supply its new fence. This
/// path does not append another start event; it binds the reclaimed directive to the existing
/// running state, repeats the provider effect under the same semantic attempt, and atomically
/// acknowledges the new fence with the result.
///
/// # Errors
/// Rejects a state that is not the exact running attempt named by the directive, plan/request
/// drift, a stale reclaim fence, provider/protocol failures that cannot be represented, or
/// settlement failure.
#[allow(clippy::too_many_arguments, reason = "effect owners and reclaimed fence stay explicit")]
pub async fn resume_model_attempt(
    journal: &mut SqliteJournal,
    provider: &dyn ModelProvider,
    state: &DebuggerState,
    plan: &ModelAnalysisPlan,
    manifest: &TraceSelectionManifest,
    debugger_limits: crate::DebuggerLimits,
    claim: ModelDirectiveClaim,
    settlement_ids: TransitionIds,
    cancellation: CancellationToken,
) -> Result<ResumedModelAttemptExecution, DebuggerError> {
    let directive = claim.directive();
    let model = state
        .model()
        .ok_or_else(|| binding("reclaimed model directive has no durable model plan"))?;
    if state.phase() != DebuggerPhase::ModelRunning
        || directive.job_id() != state.job_id()
        || directive.model_id() != plan.id()
        || directive.plan_digest() != plan.digest()
        || directive.request_digest() != plan.request_digest()
        || model.id() != plan.id()
        || model.plan_digest() != plan.digest()
        || model.request_digest() != plan.request_digest()
        || model.budget() != plan.budget()
        || !running_directive_matches(model.state(), directive)
    {
        return Err(binding(
            "running model state, plan, and reclaimed directive differ",
        ));
    }
    let (settled, outcome) = settle_model_attempt_effect(
        journal,
        provider,
        state,
        plan,
        manifest,
        debugger_limits,
        claim,
        settlement_ids,
        cancellation,
    )
    .await?;
    Ok(ResumedModelAttemptExecution { settled, outcome })
}

#[allow(clippy::too_many_arguments, reason = "effect owners and exact fence stay explicit")]
async fn settle_model_attempt_effect(
    journal: &mut SqliteJournal,
    provider: &dyn ModelProvider,
    state: &DebuggerState,
    plan: &ModelAnalysisPlan,
    manifest: &TraceSelectionManifest,
    debugger_limits: crate::DebuggerLimits,
    claim: ModelDirectiveClaim,
    settlement_ids: TransitionIds,
    cancellation: CancellationToken,
) -> Result<(CommittedDebuggerTransition, ModelAttemptOutcome), DebuggerError> {
    let attempt = claim.directive().attempt();
    let prior_usage = accumulated_usage(state);
    let continuation = retry_continuation(state, attempt);
    let result = run_model_analysis_with_usage(
        provider,
        plan,
        manifest,
        debugger_limits,
        cancellation,
        prior_usage,
        continuation.as_ref(),
    )
    .await;
    let (settlement_kind, outcome) = match result {
        Ok(success) => proposal_settlement(plan, attempt, &success),
        Err(error) => {
            let failure = model_failure(plan, attempt, &error)?;
            let outcome = if failure.code() == ModelAttemptFailureCode::Cancelled {
                ModelAttemptOutcome::Cancelled
            } else {
                ModelAttemptOutcome::Failed(failure.clone())
            };
            (DebuggerCommandKind::RecordModelFailure { failure }, outcome)
        }
    };
    let settlement_command = command(state, settlement_ids, settlement_kind)?;
    let settlement_transition = decide(Some(state), &settlement_command)?;
    let settlement_batch =
        commit_debugger_settlement(journal, &settlement_command, &settlement_transition, claim)?;
    Ok((
        CommittedDebuggerTransition::new(
            settlement_batch,
            settlement_transition.state().clone(),
        ),
        outcome,
    ))
}

fn running_directive_matches(state: &ModelWorkState, directive: ModelDirective) -> bool {
    match state {
        ModelWorkState::Running { attempt, started_at_tick } => {
            *attempt == directive.attempt()
                && directive.schedule().is_none()
                && if *attempt == 1 {
                    directive.not_before_tick() == 0
                } else {
                    *started_at_tick >= directive.not_before_tick()
                }
        }
        ModelWorkState::RunningOnClock {
            attempt,
            started_at,
            basis,
            schedule,
        } => {
            let admitted = match (*basis, *schedule) {
                (ModelStartBasis::Immediate, None) => {
                    directive.attempt() == 1 && directive.not_before_tick() == 0
                }
                (ModelStartBasis::LegacyTick, None) => {
                    directive.attempt() > 1
                        && started_at.tick_millis() >= directive.not_before_tick()
                }
                (ModelStartBasis::Scheduled | ModelStartBasis::RestartRebased, Some(value)) => {
                    directive.schedule() == Some(value)
                        && value.admission(*started_at) == Some(*basis)
                }
                _ => false,
            };
            *attempt == directive.attempt() && directive.schedule() == *schedule && admitted
        }
        ModelWorkState::Pending { .. }
        | ModelWorkState::PendingOnClock { .. }
        | ModelWorkState::AwaitingRetry { .. }
        | ModelWorkState::Validated { .. }
        | ModelWorkState::Rejected { .. } => false,
    }
}

/// Schedules the exact next attempt after a retryable durable failure.
///
/// # Errors
/// Rejects non-retry state, zero/excess delay, authority-clock overflow, or C0 conflict.
pub fn schedule_model_retry(
    journal: &mut SqliteJournal,
    state: &DebuggerState,
    scheduled_at: AuthorityInstant,
    delay_millis: u64,
    ids: TransitionIds,
) -> Result<CommittedDebuggerTransition, DebuggerError> {
    let model = state.model().ok_or_else(|| binding("retry has no model plan"))?;
    let ModelWorkState::AwaitingRetry { attempt, .. } = model.state() else {
        return Err(binding("retry scheduling requires a retryable settled failure"));
    };
    if delay_millis == 0 || delay_millis > model.retry_policy().max_delay_millis() {
        return Err(DebuggerError::numbers(
            DebuggerErrorKind::Budget,
            DebuggerOperation::RunModelAnalysis,
            DebuggerRecovery::CorrectInput,
            "model retry delay is zero or exceeds the frozen policy",
            model.retry_policy().max_delay_millis(),
            delay_millis,
        ));
    }
    let next_attempt =
        (*attempt).checked_add(1).ok_or_else(|| binding("model retry attempt overflowed"))?;
    let schedule = ModelRetrySchedule::new(scheduled_at, delay_millis)?;
    let command = command(
        state,
        ids,
        DebuggerCommandKind::ScheduleModelRetryOnClock {
            model_id: model.id(),
            next_attempt,
            schedule,
        },
    )?;
    let transition = decide(Some(state), &command)?;
    let batch = commit_debugger_transition(journal, &command, &transition)?;
    Ok(CommittedDebuggerTransition::new(batch, transition.state().clone()))
}

/// Replaces caller-owned retry stopping and delay bounds without replacing model work.
///
/// A broader policy can reopen a recoverable rejection with its exact failure and native
/// continuation intact. A narrower stopping policy settles an unscheduled retry. An amendment
/// cannot invalidate a directive that is already durable.
///
/// # Errors
/// Rejects an unrelated model, an illegal lifecycle point, an invalidation of an existing
/// directive, or a C0 conflict.
pub fn amend_model_retry_policy(
    journal: &mut SqliteJournal,
    state: &DebuggerState,
    retry_policy: ModelRetryPolicy,
    ids: TransitionIds,
) -> Result<CommittedDebuggerTransition, DebuggerError> {
    let model = state.model().ok_or_else(|| binding("retry amendment has no model plan"))?;
    let command = command(
        state,
        ids,
        DebuggerCommandKind::AmendModelRetryPolicy {
            model_id: model.id(),
            retry_policy,
        },
    )?;
    let transition = decide(Some(state), &command)?;
    let batch = commit_debugger_transition(journal, &command, &transition)?;
    Ok(CommittedDebuggerTransition::new(batch, transition.state().clone()))
}

fn proposal_settlement(
    plan: &ModelAnalysisPlan,
    attempt: u16,
    success: &ModelRunSuccess,
) -> (DebuggerCommandKind, ModelAttemptOutcome) {
    (
        DebuggerCommandKind::RecordModelProposal {
            model_id: plan.id(),
            attempt,
            proposal_digest: success.proposal().digest(),
            output_digest: success.output_digest(),
            output_bytes: success.output_bytes(),
            event_count: success.event_count(),
            input_tokens: success.input_tokens(),
            output_tokens: success.output_tokens(),
            total_tokens: success.total_tokens(),
        },
        ModelAttemptOutcome::Proposal(success.proposal().clone()),
    )
}

fn model_failure(
    plan: &ModelAnalysisPlan,
    attempt: u16,
    error: &ModelRunFailure,
) -> Result<ModelAttemptFailure, DebuggerError> {
    if error.context().profile_id() != plan.request().profile_id()
        || error.context().profile_revision() != plan.request().profile_revision()
    {
        return Err(binding("model failure context differs from the frozen provider profile"));
    }
    ModelAttemptFailure::observed(
        plan.id(),
        attempt,
        error.code(),
        error.context().clone(),
        error.diagnostic_digest(),
        error.event_count(),
        error.output_bytes(),
        error.input_tokens(),
        error.output_tokens(),
        error.total_tokens(),
    )
}

fn accumulated_usage(state: &DebuggerState) -> ModelPriorUsage {
    let mut usage = ModelPriorUsage::default();
    for observation in state.model_attempts() {
        let (events, output_bytes, input_tokens, output_tokens, total_tokens) =
            match observation.result() {
                ModelAttemptResult::Proposal {
                    output_bytes,
                    event_count,
                    input_tokens,
                    output_tokens,
                    total_tokens,
                    ..
                } => (
                    *event_count,
                    *output_bytes,
                    *input_tokens,
                    *output_tokens,
                    *total_tokens,
                ),
                ModelAttemptResult::Failure(failure) => (
                    failure.event_count(),
                    failure.output_bytes(),
                    failure.input_tokens(),
                    failure.output_tokens(),
                    failure.total_tokens(),
                ),
            };
        usage.events = usage.events.saturating_add(events);
        usage.output_bytes = usage.output_bytes.saturating_add(output_bytes);
        usage.input_tokens = usage.input_tokens.saturating_add(input_tokens);
        usage.output_tokens = usage.output_tokens.saturating_add(output_tokens);
        usage.total_tokens = usage.total_tokens.saturating_add(total_tokens);
    }
    usage
}

fn retry_continuation(
    state: &DebuggerState,
    attempt: u16,
) -> Option<peritus_model_protocol::Continuation> {
    let previous = attempt.checked_sub(1)?;
    let observation = state.model_attempts().last()?;
    if observation.attempt() != previous {
        return None;
    }
    let ModelAttemptResult::Failure(failure) = observation.result() else {
        return None;
    };
    let context = failure.context()?;
    context
        .recovery()
        .permits_same_plan_retry()
    .then(|| context.continuation().cloned())
    .flatten()
}

fn command(
    state: &DebuggerState,
    ids: TransitionIds,
    kind: DebuggerCommandKind,
) -> Result<DebuggerCommand, DebuggerError> {
    DebuggerCommand::new(
        ids.command_id(),
        ids.event_id(),
        state.job_id(),
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.query_digest(),
        kind,
    )
}

fn binding(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Binding,
        DebuggerOperation::RunModelAnalysis,
        DebuggerRecovery::ReplayAggregate,
        detail,
    )
}
