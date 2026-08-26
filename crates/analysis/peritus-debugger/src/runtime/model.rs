//! Commit-before-provider model attempt execution and bounded retry scheduling.

use peritus_journal::SqliteJournal;
use peritus_provider_core::{CancellationToken, ModelProvider};

use crate::{
    DebuggerCommand, DebuggerCommandKind, DebuggerError, DebuggerErrorKind, DebuggerOperation,
    DebuggerPhase, DebuggerRecovery, DebuggerState, ModelAnalysisPlan, ModelAttemptFailure,
    ModelAttemptFailureCode, ModelDirectiveClaim, ModelRunSuccess, ModelWorkState,
    TraceSelectionManifest, ValidatedModelProposal, commit_debugger_claimed_transition,
    commit_debugger_settlement, commit_debugger_transition, decide, run_model_analysis,
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
    /// Provider/protocol/validation/budget failure was retained digest-only.
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
/// successful `ModelAttemptExecution::Failed` because their digest-only result is durable.
#[allow(clippy::too_many_arguments, reason = "effect owners and fences remain explicit")]
pub async fn execute_model_attempt(
    journal: &mut SqliteJournal,
    provider: &dyn ModelProvider,
    state: &DebuggerState,
    plan: &ModelAnalysisPlan,
    manifest: &TraceSelectionManifest,
    debugger_limits: crate::DebuggerLimits,
    claim: ModelDirectiveClaim,
    started_at_tick: u64,
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
        || model.retry_policy() != plan.retry_policy()
    {
        return Err(binding("model state, plan, and claimed directive differ"));
    }
    let start_command = command(
        state,
        ids.start,
        DebuggerCommandKind::MarkModelAttemptStarted {
            model_id: plan.id(),
            attempt: directive.attempt(),
            started_at_tick,
        },
    )?;
    let start_transition = decide(Some(state), &start_command)?;
    let start_batch =
        commit_debugger_claimed_transition(journal, &start_command, &start_transition, claim)?;
    let running = start_transition.state().clone();
    let started = CommittedDebuggerTransition::new(start_batch, running.clone());
    let result = run_model_analysis(provider, plan, manifest, debugger_limits, cancellation).await;
    let (settlement_kind, outcome) = match result {
        Ok(success) => proposal_settlement(plan, directive.attempt(), &success),
        Err(error) if error.kind() == DebuggerErrorKind::Cancelled => {
            let reason_digest = diagnostic_digest(&error);
            (DebuggerCommandKind::CancelJob { reason_digest }, ModelAttemptOutcome::Cancelled)
        }
        Err(error) => {
            let failure = model_failure(plan, directive.attempt(), &error)?;
            (
                DebuggerCommandKind::RecordModelFailure { failure },
                ModelAttemptOutcome::Failed(failure),
            )
        }
    };
    let settlement_command = command(&running, ids.settlement, settlement_kind)?;
    let settlement_transition = decide(Some(&running), &settlement_command)?;
    let settlement_batch =
        commit_debugger_settlement(journal, &settlement_command, &settlement_transition, claim)?;
    Ok(ModelAttemptExecution {
        started,
        settled: CommittedDebuggerTransition::new(
            settlement_batch,
            settlement_transition.state().clone(),
        ),
        outcome,
    })
}

/// Schedules the exact next attempt after a retryable durable failure.
///
/// # Errors
/// Rejects non-retry state, zero/excess delay, monotonic-tick overflow, or C0 conflict.
pub fn schedule_model_retry(
    journal: &mut SqliteJournal,
    state: &DebuggerState,
    now_tick: u64,
    delay_ticks: u64,
    ids: TransitionIds,
) -> Result<CommittedDebuggerTransition, DebuggerError> {
    let model = state.model().ok_or_else(|| binding("retry has no model plan"))?;
    let ModelWorkState::AwaitingRetry { attempt, .. } = model.state() else {
        return Err(binding("retry scheduling requires a retryable settled failure"));
    };
    if delay_ticks == 0 || delay_ticks > model.retry_policy().max_delay_ticks() {
        return Err(DebuggerError::numbers(
            DebuggerErrorKind::Budget,
            DebuggerOperation::RunModelAnalysis,
            DebuggerRecovery::CorrectInput,
            "model retry delay is zero or exceeds the frozen policy",
            model.retry_policy().max_delay_ticks(),
            delay_ticks,
        ));
    }
    let next_attempt =
        attempt.checked_add(1).ok_or_else(|| binding("model retry attempt overflowed"))?;
    let not_before_tick = now_tick
        .checked_add(delay_ticks)
        .ok_or_else(|| binding("model retry scheduling tick overflowed"))?;
    let command = command(
        state,
        ids,
        DebuggerCommandKind::ScheduleModelRetry {
            model_id: model.id(),
            next_attempt,
            not_before_tick,
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
    error: &DebuggerError,
) -> Result<ModelAttemptFailure, DebuggerError> {
    let code = match error.kind() {
        DebuggerErrorKind::Budget => ModelAttemptFailureCode::BudgetExceeded,
        DebuggerErrorKind::ModelRejected => ModelAttemptFailureCode::InvalidProposal,
        DebuggerErrorKind::ModelProtocol => ModelAttemptFailureCode::MalformedStream,
        DebuggerErrorKind::Cancelled => ModelAttemptFailureCode::Cancelled,
        _ => ModelAttemptFailureCode::ProviderStream,
    };
    let retryable = error.recovery() == DebuggerRecovery::Retry
        && !matches!(
            code,
            ModelAttemptFailureCode::BudgetExceeded
                | ModelAttemptFailureCode::InvalidProposal
                | ModelAttemptFailureCode::Cancelled
        );
    ModelAttemptFailure::new(plan.id(), attempt, code, retryable, diagnostic_digest(error), 0, 0)
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

fn diagnostic_digest(error: &DebuggerError) -> peritus_types::Sha256Digest {
    let mut bytes = b"peritus.debugger.model-failure.v1\0".to_vec();
    bytes.extend_from_slice(
        format!("{:?}:{:?}:{:?}", error.kind(), error.operation(), error.recovery()).as_bytes(),
    );
    bytes.extend_from_slice(error.detail().as_bytes());
    peritus_codec::sha256(&bytes)
}

fn binding(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Binding,
        DebuggerOperation::RunModelAnalysis,
        DebuggerRecovery::ReplayAggregate,
        detail,
    )
}
