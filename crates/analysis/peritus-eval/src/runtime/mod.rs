//! Narrow effect orchestration over existing C0 owners.

mod artifact;
mod publication;
mod recovery;

use peritus_journal::{CommittedBatch, SqliteJournal};
use peritus_types::{CommandId, EventId, Sha256Digest};

pub use artifact::{
    FinalizedEvaluationArtifact, commit_report_ready, finalize_report_artifact,
    stage_and_commit_report,
};
pub use publication::{
    PublicationExecution, cancel_claimed_publication, publish_claimed_report,
};
pub use recovery::{
    DeliveryRecoveryObservation, EvaluationDeliveryTarget, EvaluationRecoveryDecision,
    RecoveryObservation, decide_recovery, decide_recovery_with_delivery,
};

use crate::{
    AnalysisSafePoint, EvaluationCommand, EvaluationCommandKind, EvaluationError,
    EvaluationErrorKind, EvaluationOperation, EvaluationRecovery, EvaluationState,
    EvaluationTransition, ExecutionDirectiveClaim, ExecutionDirectiveKind,
    FrozenEvaluationProfile, RetryAttemptRecord, RetryIntent, RolloutStatus,
    commit_evaluation_claimed_transition, commit_evaluation_settlement,
    commit_evaluation_transition, decide,
};

/// Caller-reserved command/event identities for one exact transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionIds {
    command_id: CommandId,
    event_id: EventId,
}

impl TransitionIds {
    /// Binds caller-reserved C0 identities.
    #[must_use]
    pub const fn new(command_id: CommandId, event_id: EventId) -> Self {
        Self { command_id, event_id }
    }
    /// Command identity.
    #[must_use]
    pub const fn command_id(self) -> CommandId {
        self.command_id
    }
    /// Event identity.
    #[must_use]
    pub const fn event_id(self) -> EventId {
        self.event_id
    }
}

/// One committed C0 batch paired with its exact successor state.
#[derive(Debug)]
pub struct CommittedEvaluationTransition {
    batch: CommittedBatch,
    state: EvaluationState,
}

impl CommittedEvaluationTransition {
    pub(crate) const fn new(batch: CommittedBatch, state: EvaluationState) -> Self {
        Self { batch, state }
    }
    /// Opaque C0 commit observation.
    #[must_use]
    pub const fn batch(&self) -> &CommittedBatch {
        &self.batch
    }
    /// Exact successor state.
    #[must_use]
    pub const fn state(&self) -> &EvaluationState {
        &self.state
    }
    /// Consumes the complete result.
    #[must_use]
    pub fn into_parts(self) -> (CommittedBatch, EvaluationState) {
        (self.batch, self.state)
    }
}

/// Production E3 runtime facade over the single C0 journal owner.
pub struct EvaluationRuntime<'a> {
    journal: &'a mut SqliteJournal,
}

impl<'a> EvaluationRuntime<'a> {
    /// Borrows the externally owned journal mutably for this runtime turn.
    #[must_use]
    pub const fn new(journal: &'a mut SqliteJournal) -> Self {
        Self { journal }
    }

    /// Commits an already pure-decided ordinary transition.
    ///
    /// # Errors
    /// Returns the stable E3 failure when C0 rejects or cannot commit the transition.
    pub fn commit(
        &mut self,
        command: &EvaluationCommand,
        transition: &EvaluationTransition,
    ) -> Result<CommittedEvaluationTransition, EvaluationError> {
        let batch = commit_evaluation_transition(self.journal, command, transition)?;
        Ok(CommittedEvaluationTransition::new(batch, transition.state().clone()))
    }

    /// Commits the exact claimed initial or retained-retry attempt before external execution.
    ///
    /// # Errors
    /// Rejects directive/state drift, a mismatched retry identity, stale fences, or C0 failure.
    pub fn start_claimed_rollout(
        &mut self,
        state: &EvaluationState,
        claim: ExecutionDirectiveClaim,
        started_at_tick: u64,
        ids: TransitionIds,
    ) -> Result<CommittedEvaluationTransition, EvaluationError> {
        let directive = *claim.directive();
        let progress = state
            .rollout(directive.rollout_id())
            .ok_or_else(|| runtime_binding("execution directive references an unknown rollout"))?;
        if directive.campaign_id() != state.campaign_id() {
            return Err(runtime_binding("execution directive campaign differs from state"));
        }
        let kind = match directive.kind() {
            ExecutionDirectiveKind::Execute { request_digest } => {
                let attempt = progress
                    .attempts_retained()
                    .checked_add(1)
                    .ok_or_else(|| runtime_binding("legacy execution attempt overflowed"))?;
                if request_digest != progress.binding().request_digest()
                    || !matches!(progress.status(), RolloutStatus::Scheduled { .. })
                {
                    return Err(runtime_binding("legacy execution directive differs from state"));
                }
                EvaluationCommandKind::StartRollout {
                    rollout_id: directive.rollout_id(),
                    attempt,
                    started_at_tick,
                }
            }
            ExecutionDirectiveKind::ExecuteAttempt { request_digest, attempt, retry: None } => {
                if request_digest != progress.binding().request_digest()
                    || attempt != progress.attempts_retained().checked_add(1).unwrap_or(0)
                    || !matches!(progress.status(), RolloutStatus::Scheduled { .. })
                {
                    return Err(runtime_binding("initial execution attempt differs from state"));
                }
                EvaluationCommandKind::StartRollout {
                    rollout_id: directive.rollout_id(),
                    attempt,
                    started_at_tick,
                }
            }
            ExecutionDirectiveKind::ExecuteAttempt {
                request_digest,
                attempt,
                retry: Some(retry),
            } => {
                if request_digest != progress.binding().request_digest()
                    || attempt != retry.next_attempt()
                    || progress.status() != (RolloutStatus::RetryPending { retry })
                {
                    return Err(runtime_binding("retained retry directive differs from state"));
                }
                EvaluationCommandKind::StartRetryRollout {
                    rollout_id: directive.rollout_id(),
                    retry,
                    started_at_tick,
                }
            }
            ExecutionDirectiveKind::Cancel => {
                return Err(runtime_binding("cancellation directive cannot start execution"));
            }
        };
        let command = command(state, ids, kind)?;
        let transition = decide(Some(state), &command)?;
        let batch =
            commit_evaluation_claimed_transition(self.journal, &command, &transition, claim)?;
        Ok(CommittedEvaluationTransition::new(batch, transition.state().clone()))
    }

    /// Retains one retryable attempt and atomically replaces its claim with the frozen retry.
    ///
    /// # Errors
    /// Rejects exhausted finite policy, profile/state drift, missing artifacts, claim drift, or C0
    /// failure. Persistent policy stops only at the attempt representation boundary or cancellation.
    pub fn retain_retryable_attempt(
        &mut self,
        state: &EvaluationState,
        profile: &FrozenEvaluationProfile,
        retained: RetryAttemptRecord,
        claim: ExecutionDirectiveClaim,
        ids: TransitionIds,
    ) -> Result<CommittedEvaluationTransition, EvaluationError> {
        if profile.digest() != state.profile_digest() {
            return Err(runtime_binding("retry profile differs from campaign state"));
        }
        let retry = RetryIntent::new(retained, profile.digest(), profile.retry())?;
        let rollout_id = claim.directive().rollout_id();
        if retry.retained().rollout_id() != rollout_id {
            return Err(runtime_binding("retry evidence belongs to another logical rollout"));
        }
        let command = command(
            state,
            ids,
            EvaluationCommandKind::RetainRetryableAttemptAndRetry { rollout_id, retry },
        )?;
        let transition = decide(Some(state), &command)?;
        let batch = commit_evaluation_settlement(self.journal, &command, &transition, claim)?;
        Ok(CommittedEvaluationTransition::new(batch, transition.state().clone()))
    }

    /// Retains a resumable owner-bound analysis checkpoint.
    ///
    /// # Errors
    /// Rejects phase, ownership, artifact, fence, or C0 commit failures.
    pub fn record_analysis_safe_point(
        &mut self,
        state: &EvaluationState,
        safe_point: AnalysisSafePoint,
        ids: TransitionIds,
    ) -> Result<CommittedEvaluationTransition, EvaluationError> {
        self.commit_kind(
            state,
            ids,
            EvaluationCommandKind::RecordAnalysisSafePoint { safe_point },
        )
    }

    /// Durably suspends a nonterminal campaign at its current safe boundary.
    ///
    /// # Errors
    /// Rejects unsafe analysis suspension, invalid phases, stale fences, or C0 failures.
    pub fn suspend_campaign(
        &mut self,
        state: &EvaluationState,
        reason_digest: Sha256Digest,
        analysis_safe_point: Option<AnalysisSafePoint>,
        ids: TransitionIds,
    ) -> Result<CommittedEvaluationTransition, EvaluationError> {
        self.commit_kind(
            state,
            ids,
            EvaluationCommandKind::SuspendCampaign { reason_digest, analysis_safe_point },
        )
    }

    /// Resumes the exact phase retained by a matching durable suspension.
    ///
    /// # Errors
    /// Rejects a mismatched reason, invalid phase, stale fence, or C0 failure.
    pub fn resume_campaign(
        &mut self,
        state: &EvaluationState,
        reason_digest: Sha256Digest,
        ids: TransitionIds,
    ) -> Result<CommittedEvaluationTransition, EvaluationError> {
        self.commit_kind(
            state,
            ids,
            EvaluationCommandKind::ResumeCampaign { reason_digest },
        )
    }

    /// Starts cancellation from any nonterminal campaign phase.
    ///
    /// # Errors
    /// Rejects duplicate cancellation, stale fences, or C0 failures.
    pub fn cancel_campaign(
        &mut self,
        state: &EvaluationState,
        reason_digest: Sha256Digest,
        ids: TransitionIds,
    ) -> Result<CommittedEvaluationTransition, EvaluationError> {
        self.commit_kind(
            state,
            ids,
            EvaluationCommandKind::CancelCampaign { reason_digest },
        )
    }

    /// Confirms active analysis cancellation and retains its final safe point.
    ///
    /// # Errors
    /// Rejects ownership, monotonicity, phase, artifact, fence, or C0 failures.
    pub fn settle_analysis_cancellation(
        &mut self,
        state: &EvaluationState,
        safe_point: AnalysisSafePoint,
        ids: TransitionIds,
    ) -> Result<CommittedEvaluationTransition, EvaluationError> {
        self.commit_kind(
            state,
            ids,
            EvaluationCommandKind::SettleAnalysisCancellation { safe_point },
        )
    }

    /// Completes cancellation only after every owned effect has been reconciled.
    ///
    /// # Errors
    /// Rejects incomplete effect settlement, stale fences, or C0 failures.
    pub fn complete_cancellation(
        &mut self,
        state: &EvaluationState,
        ids: TransitionIds,
    ) -> Result<CommittedEvaluationTransition, EvaluationError> {
        self.commit_kind(state, ids, EvaluationCommandKind::CompleteCancellation)
    }

    fn commit_kind(
        &mut self,
        state: &EvaluationState,
        ids: TransitionIds,
        kind: EvaluationCommandKind,
    ) -> Result<CommittedEvaluationTransition, EvaluationError> {
        let command = command(state, ids, kind)?;
        let transition = decide(Some(state), &command)?;
        self.commit(&command, &transition)
    }

    /// Borrows the underlying journal for C0 claim/replay composition.
    #[must_use]
    pub const fn journal(&mut self) -> &mut SqliteJournal {
        self.journal
    }
}

fn command(
    state: &EvaluationState,
    ids: TransitionIds,
    kind: EvaluationCommandKind,
) -> Result<EvaluationCommand, EvaluationError> {
    EvaluationCommand::new(
        ids.command_id(),
        ids.event_id(),
        state.campaign_id(),
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.profile_digest(),
        kind,
    )
}

const fn runtime_binding(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Binding,
        EvaluationOperation::Commit,
        EvaluationRecovery::Quarantine,
        detail,
    )
}
