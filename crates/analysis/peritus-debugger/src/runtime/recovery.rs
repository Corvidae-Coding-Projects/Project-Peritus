//! Deterministic restart decisions from durable state and owner observations.

use crate::{DebuggerPhase, DebuggerState, ModelWorkState};

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
            Some(ModelWorkState::Pending { attempt, .. }) if directive_available => {
                DebuggerRecoveryDecision::ClaimModelAttempt { attempt }
            }
            Some(ModelWorkState::AwaitingRetry { attempt, .. }) => {
                DebuggerRecoveryDecision::ScheduleModelRetry { completed_attempt: attempt }
            }
            _ => DebuggerRecoveryDecision::Quarantine,
        },
        DebuggerPhase::ModelRunning => match state.model().map(crate::ModelProgress::state) {
            Some(ModelWorkState::Running { attempt, .. }) if directive_available => {
                DebuggerRecoveryDecision::ResumeModelAttempt { attempt }
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
