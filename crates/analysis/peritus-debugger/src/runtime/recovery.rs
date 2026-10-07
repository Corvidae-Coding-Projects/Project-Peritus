//! Deterministic restart decisions from durable state and owner observations.

use peritus_policy::AuthorityInstant;

use crate::{DebuggerPhase, DebuggerState, ModelRetrySchedule, ModelWorkState};

/// Closed recovery action selected without performing an effect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DebuggerRecoveryDecision {
    /// Resume the next deterministic read-only analysis stage.
    ContinueDeterministic,
    /// Claim the durable model directive for this exact attempt.
    ClaimModelAttempt {
        /// Exact one-based attempt to claim.
        attempt: u16,
    },
    /// Evaluate an epoch-bound schedule against a current authority-clock observation.
    EvaluateModelRetry {
        /// Exact one-based attempt whose directive is pending.
        attempt: u16,
        /// Exact durable schedule to evaluate without resetting its origin.
        schedule: ModelRetrySchedule,
    },
    /// Evaluate an accepted legacy bare-tick directive.
    EvaluateLegacyModelRetry {
        /// Exact one-based attempt whose legacy directive is pending.
        attempt: u16,
        /// Legacy tick threshold, whose epoch and unit were not recorded.
        not_before_tick: u64,
    },
    /// The durable retry directive exists but its delay has not elapsed.
    AwaitModelRetry {
        /// Exact one-based pending attempt.
        attempt: u16,
    },
    /// Reclaim/resume the exact already-started attempt after its C0 lease permits it.
    ResumeModelAttempt {
        /// Exact one-based attempt to resume.
        attempt: u16,
    },
    /// Commit a bounded retry-scheduling transition.
    ScheduleModelRetry {
        /// Exact completed attempt for which a successor must be scheduled.
        completed_attempt: u16,
    },
    /// Assemble, validate, and stage the complete canonical report artifact.
    PrepareReport,
    /// Claim the durable publication directive and admit report evidence.
    ClaimPublication,
    /// Exact evidence already exists; retry only the publication settlement.
    ReconcilePublication,
    /// Durable terminal work is complete.
    Complete,
    /// Owner observations contradict the durable state and require quarantine.
    Quarantine,
}

/// Chooses the sole restart action from exact state plus read-only C0 owner observations.
///
/// `staged_artifact` means the report digest is finalized and verifies. `evidence_admitted` means
/// the exact content-derived report evidence exists. `directive_available` means the expected
/// durable outbox row is pending/claimable or already held by this recovery worker.
#[must_use]
pub fn decide_recovery(
    state: &DebuggerState,
    staged_artifact: bool,
    evidence_admitted: bool,
    directive_available: bool,
) -> DebuggerRecoveryDecision {
    match state.phase() {
        DebuggerPhase::Created | DebuggerPhase::Selected => {
            DebuggerRecoveryDecision::ContinueDeterministic
        }
        DebuggerPhase::DeterministicComplete | DebuggerPhase::ModelValidated => {
            DebuggerRecoveryDecision::PrepareReport
        }
        DebuggerPhase::ModelPending => match state.model().map(crate::ModelProgress::state) {
            Some(ModelWorkState::Pending { attempt: 1, not_before_tick: 0 })
                if directive_available =>
            {
                DebuggerRecoveryDecision::ClaimModelAttempt { attempt: 1 }
            }
            Some(ModelWorkState::Pending { attempt, not_before_tick }) => {
                DebuggerRecoveryDecision::EvaluateLegacyModelRetry {
                    attempt: *attempt,
                    not_before_tick: *not_before_tick,
                }
            }
            Some(ModelWorkState::PendingOnClock { attempt, schedule }) => {
                DebuggerRecoveryDecision::EvaluateModelRetry {
                    attempt: *attempt,
                    schedule: *schedule,
                }
            }
            Some(ModelWorkState::AwaitingRetry { attempt, .. }) => {
                DebuggerRecoveryDecision::ScheduleModelRetry { completed_attempt: *attempt }
            }
            _ => DebuggerRecoveryDecision::Quarantine,
        },
        DebuggerPhase::ModelRunning => match state.model().map(crate::ModelProgress::state) {
            Some(
                ModelWorkState::Running { attempt, .. }
                | ModelWorkState::RunningOnClock { attempt, .. },
            ) if directive_available => {
                DebuggerRecoveryDecision::ResumeModelAttempt { attempt: *attempt }
            }
            _ => DebuggerRecoveryDecision::Quarantine,
        },
        DebuggerPhase::ReportReady => {
            if !staged_artifact || !directive_available {
                DebuggerRecoveryDecision::Quarantine
            } else if evidence_admitted {
                DebuggerRecoveryDecision::ReconcilePublication
            } else {
                DebuggerRecoveryDecision::ClaimPublication
            }
        }
        DebuggerPhase::Published => {
            if staged_artifact && evidence_admitted {
                DebuggerRecoveryDecision::Complete
            } else {
                DebuggerRecoveryDecision::ReconcilePublication
            }
        }
        DebuggerPhase::Failed | DebuggerPhase::Cancelled => DebuggerRecoveryDecision::Complete,
    }
}

/// Chooses restart work after evaluating pending retry eligibility on the authority clock.
///
/// This is the execution form of [`decide_recovery`]. It uses the same schedule admission
/// function as the aggregate reducer and claimed-attempt runtime boundary.
#[must_use]
pub fn decide_recovery_on_clock(
    state: &DebuggerState,
    staged_artifact: bool,
    evidence_admitted: bool,
    directive_available: bool,
    observed_at: AuthorityInstant,
) -> DebuggerRecoveryDecision {
    match decide_recovery(
        state,
        staged_artifact,
        evidence_admitted,
        directive_available,
    ) {
        DebuggerRecoveryDecision::EvaluateModelRetry { attempt, schedule } => {
            if schedule.admission(observed_at).is_none() {
                DebuggerRecoveryDecision::AwaitModelRetry { attempt }
            } else if directive_available {
                DebuggerRecoveryDecision::ClaimModelAttempt { attempt }
            } else {
                DebuggerRecoveryDecision::Quarantine
            }
        }
        DebuggerRecoveryDecision::EvaluateLegacyModelRetry {
            attempt,
            not_before_tick,
        } => {
            if observed_at.tick_millis() < not_before_tick {
                DebuggerRecoveryDecision::AwaitModelRetry { attempt }
            } else if directive_available {
                DebuggerRecoveryDecision::ClaimModelAttempt { attempt }
            } else {
                DebuggerRecoveryDecision::Quarantine
            }
        }
        decision => decision,
    }
}
