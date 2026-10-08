//! Deterministic crash-recovery decisions from durable observations.

use crate::{EvaluationPhase, EvaluationState, RetryIntent, RolloutId, RolloutStatus};

/// Exact external facts observed during recovery; no field grants mutation authority.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent artifact, evidence, publication, and conflict observations remain explicit"
)]
pub struct RecoveryObservation {
    /// Number of outstanding schedule outbox rows.
    pub schedule_directives: u32,
    /// Number of outstanding execution outbox rows.
    pub execution_directives: u32,
    /// Whether the exact publication directive remains outstanding.
    pub publication_directive: bool,
    /// Whether the report artifact is finalized and verified.
    pub report_artifact_verified: bool,
    /// Whether exact report evidence is already admitted.
    pub report_evidence_admitted: bool,
    /// Whether external owners report an irreconcilable identity conflict.
    pub identity_conflict: bool,
}

/// Closed recovery action selected without guessing an external outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvaluationRecoveryDecision {
    /// No effect is pending; continue normal planning/scheduling.
    Continue,
    /// Redeliver existing schedule/cancellation directives.
    RedeliverScheduling,
    /// Redeliver existing execution/cancellation directives.
    RedeliverExecution,
    /// Redeliver the exact retained retry directive already committed with its predecessor ack.
    RedeliverRetainedRetry {
        /// Logical rollout that owns the retained continuation.
        rollout_id: RolloutId,
        /// Complete artifact, continuation, policy, delay, and next-attempt identity.
        retry: RetryIntent,
    },
    /// Every logical rollout is terminal; deterministic analysis may begin.
    BeginAnalysis,
    /// Report bytes must be finalized or reconciled.
    ReconcileReportArtifact,
    /// Publication directive should be retried exactly.
    RetryPublication,
    /// Evidence exists after a crash; retry the exact atomic settlement.
    ReconcileEvidenceSettlement,
    /// Cancellation routing/settlement must continue.
    ContinueCancellation,
    /// Durable suspension remains authoritative until an explicit matching resume.
    RemainSuspended,
    /// Campaign is already terminal and consistent.
    Complete,
    /// Conflicting external identities require quarantine.
    Quarantine,
}

/// Selects one exact recovery action from state and observed external ownership.
#[must_use]
pub fn decide_recovery(
    state: &EvaluationState,
    observed: RecoveryObservation,
) -> EvaluationRecoveryDecision {
    if observed.identity_conflict {
        return EvaluationRecoveryDecision::Quarantine;
    }
    if state.phase().terminal() {
        return EvaluationRecoveryDecision::Complete;
    }
    if state.phase() == EvaluationPhase::Cancelling {
        return EvaluationRecoveryDecision::ContinueCancellation;
    }
    if state.phase() == EvaluationPhase::Suspended {
        return EvaluationRecoveryDecision::RemainSuspended;
    }
    let mut retained_retries = state.rollouts().filter_map(|(rollout_id, progress)| {
        match progress.status() {
            RolloutStatus::RetryPending { retry } | RolloutStatus::RetryRunning { retry } => {
                Some((rollout_id, retry))
            }
            _ => None,
        }
    });
    if let Some((rollout_id, retry)) = retained_retries.next() {
        let Some(required_directives) = u32::try_from(retained_retries.count())
            .ok()
            .and_then(|additional| additional.checked_add(1))
        else {
            return EvaluationRecoveryDecision::Quarantine;
        };
        if observed.execution_directives < required_directives {
            return EvaluationRecoveryDecision::Quarantine;
        }
        return EvaluationRecoveryDecision::RedeliverRetainedRetry { rollout_id, retry };
    }
    if observed.schedule_directives > 0 {
        return EvaluationRecoveryDecision::RedeliverScheduling;
    }
    if observed.execution_directives > 0
        || state
            .rollouts()
            .any(|(_, value)| matches!(value.status(), RolloutStatus::Running { .. }))
    {
        return EvaluationRecoveryDecision::RedeliverExecution;
    }
    if state.phase() == EvaluationPhase::ReportReady {
        if !observed.report_artifact_verified {
            return EvaluationRecoveryDecision::ReconcileReportArtifact;
        }
        if observed.report_evidence_admitted {
            return EvaluationRecoveryDecision::ReconcileEvidenceSettlement;
        }
        if observed.publication_directive {
            return EvaluationRecoveryDecision::RetryPublication;
        }
    }
    if state.counts().complete()
        && matches!(
            state.phase(),
            EvaluationPhase::Planned | EvaluationPhase::Scheduling | EvaluationPhase::Running
        )
    {
        return EvaluationRecoveryDecision::BeginAnalysis;
    }
    EvaluationRecoveryDecision::Continue
}
