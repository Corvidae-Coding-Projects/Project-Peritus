//! Narrow effect orchestration over existing C0 owners.

mod analysis;
mod artifact;
mod publication;
mod recovery;

use peritus_journal::{CommittedBatch, OutboxId, SqliteJournal};
use peritus_types::{CommandId, EventId, Sha256Digest};

pub use analysis::{
    DurableAnalysisAdvance, FinalizedAnalysisArtifact, advance_durable_analysis,
};
pub use artifact::{
    FinalizedEvaluationArtifact, commit_report_ready, finalize_report_artifact,
    stage_and_commit_report,
};
pub use publication::{
    PublicationExecution, PublicationIntentRecovery, PublicationOwnershipReceipt,
    cancel_claimed_publication, load_publication_ownership, observe_publication_recovery,
    publish_claimed_report, reconcile_interrupted_publication, reconcile_publication_intent,
};
pub use recovery::{
    DeliveryRecoveryObservation, EvaluationDeliveryTarget, EvaluationRecoveryDecision,
    PublicationDependencyStatus, PublicationDirectiveObservation,
    PublicationRecoveryObservation, RecoveryObservation, decide_publication_recovery,
    decide_recovery, decide_recovery_with_delivery,
};

use crate::{
    AnalysisSafePoint, CommittedEvaluationOperation, EvaluationCampaignId, EvaluationCommand,
    EvaluationCommandKind, EvaluationCommitMode, EvaluationError, EvaluationErrorKind,
    EvaluationEvent, EvaluationEventKind, EvaluationOperation, EvaluationOperationReceipt,
    EvaluationRecovery, EvaluationState, EvaluationTransition, ExecutionDirective,
    ExecutionDirectiveClaim, ExecutionDirectiveKind, FrozenEvaluationProfile, RetryAttemptRecord,
    RetryIntent, RolloutStatus, commit_evaluation_claimed_transition,
    commit_evaluation_settlement, commit_evaluation_transition, decide,
    load_evaluation_operation,
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

/// One accepted E3 operation with its historical outcome and separately loaded current state.
#[derive(Debug)]
pub struct CommittedEvaluationTransition {
    operation: CommittedEvaluationOperation,
}

impl CommittedEvaluationTransition {
    pub(crate) const fn new(operation: CommittedEvaluationOperation) -> Self {
        Self { operation }
    }
    /// Opaque C0 commit observation.
    #[must_use]
    pub const fn batch(&self) -> &CommittedBatch {
        self.operation.batch()
    }
    /// Current state reconstructed independently after observing the accepted operation.
    #[must_use]
    pub const fn state(&self) -> &EvaluationState {
        self.operation.current_state()
    }
    /// Exact historical successor produced by this operation.
    #[must_use]
    pub const fn historical_state(&self) -> &EvaluationState {
        self.operation.historical_state()
    }
    /// Original request and claim receipt accepted for this operation.
    #[must_use]
    pub const fn receipt(&self) -> EvaluationOperationReceipt {
        self.operation.receipt()
    }
    /// Exact immutable event accepted for this operation.
    #[must_use]
    pub const fn event(&self) -> &EvaluationEvent {
        self.operation.event()
    }
    /// Whether this operation still produces the current campaign checkpoint.
    #[must_use]
    pub fn is_current(&self) -> bool {
        self.operation.is_current()
    }
    /// Consumes the result into its original batch and current state.
    #[must_use]
    pub fn into_parts(self) -> (CommittedBatch, EvaluationState) {
        let (batch, _, _, _, current) = self.operation.into_parts();
        (batch, current)
    }
    /// Consumes the result without discarding its historical outcome or original receipt.
    #[must_use]
    pub fn into_operation(self) -> CommittedEvaluationOperation {
        self.operation
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
        let operation = commit_evaluation_transition(self.journal, command, transition)?;
        Ok(CommittedEvaluationTransition::new(operation))
    }

    /// Commits the complete ledger boundary that authorizes deterministic analysis.
    ///
    /// # Errors
    /// Rejects incomplete conservation, stale fences, invalid phase, or C0 failure.
    pub fn start_analysis(
        &mut self,
        state: &EvaluationState,
        ids: TransitionIds,
    ) -> Result<CommittedEvaluationTransition, EvaluationError> {
        self.commit_kind(
            state,
            ids,
            EvaluationCommandKind::StartAnalysis { counts: state.counts() },
        )
    }

    /// Advances one physical analysis batch and commits its checkpoint or completed artifact.
    ///
    /// # Errors
    /// Rejects phase, owner, input, checkpoint, cancellation, artifact, or C0 transition drift.
    #[allow(
        clippy::too_many_arguments,
        reason = "durable analysis binds every owner, input, work, cancellation, and transition identity"
    )]
    pub fn advance_analysis(
        &mut self,
        artifact_store: &peritus_artifact_store::ArtifactStore,
        state: &EvaluationState,
        plan: &crate::EvaluationPlan,
        profile: &FrozenEvaluationProfile,
        ledger: &crate::RolloutLedger,
        owner: peritus_types::ActorId,
        work: crate::BootstrapBatchWork,
        cancellation: &peritus_journal::JournalCancellation,
        ids: TransitionIds,
    ) -> Result<DurableAnalysisAdvance, EvaluationError> {
        analysis::advance_durable_analysis(
            self.journal,
            artifact_store,
            state,
            plan,
            profile,
            ledger,
            owner,
            work,
            cancellation,
            ids,
        )
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
        if let Some(operation) = recover_claimed_start(
            self.journal,
            state.campaign_id(),
            &claim,
            started_at_tick,
            ids,
        )? {
            return Ok(CommittedEvaluationTransition::new(operation));
        }
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
        let operation =
            commit_evaluation_claimed_transition(self.journal, &command, &transition, claim)?;
        Ok(CommittedEvaluationTransition::new(operation))
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
        let kind = EvaluationCommandKind::RetainRetryableAttemptAndRetry { rollout_id, retry };
        if let Some(operation) = recover_claimed_operation(
            self.journal,
            state.campaign_id(),
            ids,
            &kind,
            EvaluationCommitMode::Settlement,
            claim.outbox_id()?,
            claim.fence(),
        )? {
            let directive = *claim.directive();
            let Some(progress) = operation.historical_state().rollout(rollout_id) else {
                return Err(runtime_recovery(
                    "recovered retry retention has no historical rollout",
                ));
            };
            let claim_matches = match directive.kind() {
                ExecutionDirectiveKind::Execute { request_digest } => {
                    request_digest == progress.binding().request_digest()
                        && retry.retained().attempt() == 1
                }
                ExecutionDirectiveKind::ExecuteAttempt {
                    request_digest,
                    attempt,
                    retry: None,
                } => {
                    request_digest == progress.binding().request_digest()
                        && attempt == retry.retained().attempt()
                }
                ExecutionDirectiveKind::ExecuteAttempt { retry: Some(_), .. }
                | ExecutionDirectiveKind::Cancel => false,
            };
            if directive.campaign_id() != operation.historical_state().campaign_id()
                || directive.rollout_id() != rollout_id
                || progress.status() != (RolloutStatus::RetryPending { retry })
                || !claim_matches
            {
                return Err(runtime_recovery(
                    "recovered retry retention differs from its historical rollout",
                ));
            }
            return Ok(CommittedEvaluationTransition::new(operation));
        }
        let command = command(state, ids, kind)?;
        let transition = decide(Some(state), &command)?;
        let operation = commit_evaluation_settlement(self.journal, &command, &transition, claim)?;
        Ok(CommittedEvaluationTransition::new(operation))
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
        if let Some(operation) =
            recover_ordinary_operation(self.journal, state.campaign_id(), ids, &kind)?
        {
            return Ok(CommittedEvaluationTransition::new(operation));
        }
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

pub(super) fn recover_ordinary_operation(
    journal: &SqliteJournal,
    campaign_id: EvaluationCampaignId,
    ids: TransitionIds,
    kind: &EvaluationCommandKind,
) -> Result<Option<CommittedEvaluationOperation>, EvaluationError> {
    let Some(operation) = load_evaluation_operation(journal, campaign_id, ids.command_id())? else {
        return Ok(None);
    };
    if !matches!(
        operation.receipt().mode(),
        EvaluationCommitMode::Ordinary | EvaluationCommitMode::Legacy
    ) || !operation_matches(&operation, campaign_id, ids, kind)
    {
        return Err(runtime_recovery(
            "recovered ordinary evaluation operation differs from the retry",
        ));
    }
    Ok(Some(operation))
}

pub(super) fn recover_claimed_operation(
    journal: &SqliteJournal,
    campaign_id: EvaluationCampaignId,
    ids: TransitionIds,
    kind: &EvaluationCommandKind,
    required_mode: EvaluationCommitMode,
    outbox_id: OutboxId,
    fence: u64,
) -> Result<Option<CommittedEvaluationOperation>, EvaluationError> {
    let Some(operation) = load_evaluation_operation(journal, campaign_id, ids.command_id())? else {
        return Ok(None);
    };
    validate_recovered_claim(
        operation.receipt(),
        required_mode,
        outbox_id,
        fence,
    )?;
    if !operation_matches(&operation, campaign_id, ids, kind) {
        return Err(runtime_recovery(
            "recovered claim-bound evaluation operation differs from the retry",
        ));
    }
    Ok(Some(operation))
}

fn recover_claimed_start(
    journal: &SqliteJournal,
    campaign_id: EvaluationCampaignId,
    claim: &ExecutionDirectiveClaim,
    started_at_tick: u64,
    ids: TransitionIds,
) -> Result<Option<CommittedEvaluationOperation>, EvaluationError> {
    let Some(operation) = load_evaluation_operation(journal, campaign_id, ids.command_id())? else {
        return Ok(None);
    };
    validate_recovered_claim(
        operation.receipt(),
        EvaluationCommitMode::Claimed,
        claim.outbox_id()?,
        claim.fence(),
    )?;
    if operation.event().id() != ids.event_id()
        || operation.event().command_id() != ids.command_id()
        || operation.event().campaign_id() != campaign_id
        || !recovered_start_matches(
            operation.event().kind(),
            operation.historical_state(),
            *claim.directive(),
            started_at_tick,
        )
    {
        return Err(runtime_recovery(
            "recovered rollout start differs from the exact retry request",
        ));
    }
    Ok(Some(operation))
}

fn recovered_start_matches(
    event: &EvaluationEventKind,
    state: &EvaluationState,
    directive: ExecutionDirective,
    started_at_tick: u64,
) -> bool {
    if directive.campaign_id() != state.campaign_id() {
        return false;
    }
    let Some(progress) = state.rollout(directive.rollout_id()) else {
        return false;
    };
    let EvaluationEventKind::Accepted(kind) = event;
    match (directive.kind(), kind) {
        (
            ExecutionDirectiveKind::Execute { request_digest },
            EvaluationCommandKind::StartRollout {
                rollout_id,
                attempt,
                started_at_tick: observed_tick,
            },
        )
        | (
            ExecutionDirectiveKind::ExecuteAttempt {
                request_digest,
                attempt: _,
                retry: None,
            },
            EvaluationCommandKind::StartRollout {
                rollout_id,
                attempt,
                started_at_tick: observed_tick,
            },
        ) => {
            *rollout_id == directive.rollout_id()
                && *observed_tick == started_at_tick
                && request_digest == progress.binding().request_digest()
                && progress.status() == (RolloutStatus::Running { attempt: *attempt })
                && match directive.kind() {
                    ExecutionDirectiveKind::ExecuteAttempt {
                        attempt: directive_attempt,
                        ..
                    } => directive_attempt == *attempt,
                    ExecutionDirectiveKind::Execute { .. } => true,
                    ExecutionDirectiveKind::Cancel => false,
                }
        }
        (
            ExecutionDirectiveKind::ExecuteAttempt {
                request_digest,
                attempt,
                retry: Some(retry),
            },
            EvaluationCommandKind::StartRetryRollout {
                rollout_id,
                retry: observed_retry,
                started_at_tick: observed_tick,
            },
        ) => {
            *rollout_id == directive.rollout_id()
                && *observed_tick == started_at_tick
                && request_digest == progress.binding().request_digest()
                && attempt == retry.next_attempt()
                && retry == *observed_retry
                && progress.status() == (RolloutStatus::RetryRunning { retry })
        }
        _ => false,
    }
}

fn operation_matches(
    operation: &CommittedEvaluationOperation,
    campaign_id: EvaluationCampaignId,
    ids: TransitionIds,
    kind: &EvaluationCommandKind,
) -> bool {
    operation.event().id() == ids.event_id()
        && operation.event().command_id() == ids.command_id()
        && operation.event().campaign_id() == campaign_id
        && matches!(
            operation.event().kind(),
            EvaluationEventKind::Accepted(observed) if observed == kind
        )
}

fn validate_recovered_claim(
    receipt: EvaluationOperationReceipt,
    required_mode: EvaluationCommitMode,
    outbox_id: OutboxId,
    fence: u64,
) -> Result<(), EvaluationError> {
    if receipt.mode() == EvaluationCommitMode::Legacy {
        return Ok(());
    }
    let Some(original) = receipt.original_claim() else {
        return Err(runtime_recovery(
            "recovered evaluation effect has no retained original claim",
        ));
    };
    if receipt.mode() != required_mode
        || original.outbox_id() != outbox_id
        || fence < original.fence()
    {
        return Err(runtime_recovery(
            "recovered evaluation effect differs from the supplied claim authority",
        ));
    }
    Ok(())
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

const fn runtime_recovery(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Recovery,
        EvaluationOperation::Recover,
        EvaluationRecovery::Quarantine,
        detail,
    )
}
