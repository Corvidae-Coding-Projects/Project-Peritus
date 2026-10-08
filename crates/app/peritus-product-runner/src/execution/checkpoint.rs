//! Exact candidate observation at every material product-run boundary.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceDependencies, EvidenceRecord,
    EvidenceStatus, QualificationEvidence, RunSettlement, SettlementCause, SettlementReducer,
};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

use super::{ConversationView, ProductRunInput};
use crate::{
    ProductRunnerError, ProductRunnerErrorKind, candidate::CandidateBaseline,
    developer_tools::ToolCheckpointBoundary,
};

/// Cloneable candidate recorder shared with synchronous developer-tool execution.
#[derive(Clone)]
pub struct CandidateRecorder {
    root: PathBuf,
    baseline: CandidateBaseline,
    run_id: RunId,
    workspace_id: WorkspaceId,
    cancelled: Arc<AtomicBool>,
    provider_cancellation: peritus_provider_core::CancellationToken,
    state: Arc<Mutex<RecorderState>>,
}

#[derive(Clone, Copy)]
struct RecorderState {
    // Only a fully checked replacement is published under the lock. A failed observation or
    // panic cannot leave sequence, context, and reducer at different transaction boundaries.
    reducer: SettlementReducer,
    next_sequence: u64,
    external_effect_observed: bool,
    execution_context: Option<Sha256Digest>,
}

/// Evidence acquired at the candidate boundary being recorded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CheckpointEvidence {
    None,
    Gates { satisfied: bool, execution_context: Sha256Digest },
    Obligations(bool),
    ObligationContractChanged,
    Review(bool),
    ExternalEffect,
}

#[derive(Clone, Copy)]
enum ObservationOwner {
    Active,
    Finalization,
}

impl CandidateRecorder {
    #[cfg(test)]
    pub(super) fn new(
        root: &Path,
        baseline: CandidateBaseline,
        run_id: RunId,
        workspace_id: WorkspaceId,
        prior: Option<&CandidateCheckpoint>,
        retain_external_effect: bool,
    ) -> Result<Self, ProductRunnerError> {
        Self::new_inner(
            root.to_owned(),
            baseline,
            run_id,
            workspace_id,
            prior,
            retain_external_effect,
            Arc::new(AtomicBool::new(false)),
            peritus_provider_core::CancellationToken::new(),
        )
    }

    pub(super) fn for_input(
        input: &ProductRunInput,
        baseline: CandidateBaseline,
        prior: Option<&CandidateCheckpoint>,
    ) -> Result<Self, ProductRunnerError> {
        Self::new_inner(
            input.workspace_root.clone(),
            baseline,
            input.run_id,
            input.workspace_id,
            prior,
            input.delivery_scope.allows_external_effects(),
            Arc::clone(&input.cancelled),
            input.provider_cancellation.clone(),
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "candidate construction binds the complete retained capture boundary"
    )]
    fn new_inner(
        root: PathBuf,
        baseline: CandidateBaseline,
        run_id: RunId,
        workspace_id: WorkspaceId,
        prior: Option<&CandidateCheckpoint>,
        retain_external_effect: bool,
        cancelled: Arc<AtomicBool>,
        provider_cancellation: peritus_provider_core::CancellationToken,
    ) -> Result<Self, ProductRunnerError> {
        let mut reducer = SettlementReducer::new();
        if let Some(checkpoint) = prior.copied() {
            if checkpoint.identity().run_id() != run_id
                || checkpoint.identity().workspace_id() != workspace_id
            {
                return Err(ProductRunnerError::new(
                    ProductRunnerErrorKind::InvalidPrecondition,
                    "restore candidate checkpoint",
                    "resume checkpoint belongs to another run or workspace",
                ));
            }
            reducer.observe(checkpoint).map_err(invariant)?;
        }
        Ok(Self {
            root,
            baseline,
            run_id,
            workspace_id,
            cancelled,
            provider_cancellation,
            state: Arc::new(Mutex::new(RecorderState {
                reducer,
                next_sequence: prior
                    .map_or(0, |checkpoint| checkpoint.identity().checkpoint_sequence()),
                external_effect_observed: retain_external_effect && prior.is_some(),
                // An in-memory continuation remains in the process that observed this context.
                // Durable decode clears the context before it reaches this recorder, so a real
                // process restart still reacquires effectful gate evidence.
                execution_context: prior
                    .and_then(|checkpoint| checkpoint.identity().execution_digest()),
            })),
        })
    }

    pub(crate) fn tool_observer(
        &self,
        conversation: Arc<dyn ConversationView>,
    ) -> Arc<dyn Fn(ToolCheckpointBoundary) -> Result<(), String> + Send + Sync> {
        let recorder = self.clone();
        Arc::new(move |boundary| {
            let revision = conversation.incorporated_revision();
            match boundary {
                ToolCheckpointBoundary::BeforeMutation { path, kind } => conversation
                    .checkpoint_before_workspace_mutation(Path::new(&path), kind)
                    .map_err(|detail| {
                        ProductRunnerError::new(
                            ProductRunnerErrorKind::InvalidPrecondition,
                            "capture user checkpoint before workspace mutation",
                            detail,
                        )
                    })
                    .map(|()| None),
                ToolCheckpointBoundary::Mutation { path, kind, owned_postchange } => conversation
                    .seal_workspace_mutation_checkpoint(Path::new(&path), kind, owned_postchange)
                    .map_err(|detail| {
                        ProductRunnerError::new(
                            ProductRunnerErrorKind::InvalidPrecondition,
                            "seal user checkpoint after workspace mutation",
                            detail,
                        )
                    })
                    .and_then(|()| {
                        recorder.record(CandidateStage::Changed, revision, CheckpointEvidence::None)
                    }),
                ToolCheckpointBoundary::BaselineMutation {
                    path,
                    kind,
                    baseline,
                    owned_postchange,
                } => conversation
                    .checkpoint_workspace_mutation_from_baseline(
                        Path::new(&path),
                        kind,
                        &baseline,
                    )
                    .map_err(|detail| {
                        ProductRunnerError::new(
                            ProductRunnerErrorKind::InvalidPrecondition,
                            "publish retained command baseline as a user checkpoint",
                            detail,
                        )
                    })
                    .and_then(|()| {
                        conversation
                            .seal_workspace_mutation_checkpoint(
                                Path::new(&path),
                                kind,
                                owned_postchange,
                            )
                            .map_err(|detail| {
                                ProductRunnerError::new(
                                    ProductRunnerErrorKind::InvalidPrecondition,
                                    "seal user checkpoint after command mutation",
                                    detail,
                                )
                            })
                    })
                    .and_then(|()| {
                        recorder.record(CandidateStage::Changed, revision, CheckpointEvidence::None)
                    }),
                ToolCheckpointBoundary::Verification => {
                    recorder.record(CandidateStage::SelfChecked, revision, CheckpointEvidence::None)
                }
                ToolCheckpointBoundary::ExternalEffect => {
                    recorder.mark_external_effect().map_err(|error| error.to_string())?;
                    recorder.record(
                        CandidateStage::Changed,
                        revision,
                        CheckpointEvidence::ExternalEffect,
                    )
                }
            }
            .map(|_| ())
            .map_err(|error| error.to_string())
        })
    }

    pub(super) fn record(
        &self,
        requested_stage: CandidateStage,
        conversation_revision: u64,
        acquired: CheckpointEvidence,
    ) -> Result<Option<CandidateCheckpoint>, ProductRunnerError> {
        self.record_observation(
            requested_stage,
            conversation_revision,
            acquired,
            None,
            ObservationOwner::Active,
        )
            .map(|(checkpoint, _)| checkpoint)
    }

    pub(crate) fn candidate_is_current(
        &self,
        expected: CandidateIdentity,
        conversation_revision: u64,
    ) -> Result<bool, ProductRunnerError> {
        let recorder_state = self.lock()?;
        recorder_state.ensure_active()?;
        self.check_capture_cancelled()?;
        let (content, repository) = self.capture_candidate_axes()?;
        self.check_capture_cancelled()?;
        let matches = candidate_material_matches(
            expected,
            self.run_id,
            self.workspace_id,
            content,
            repository,
            conversation_revision,
        );
        drop(recorder_state);
        Ok(matches)
    }

    pub(super) fn record_gates_for_candidate(
        &self,
        expected: CandidateIdentity,
        conversation_revision: u64,
        requested_stage: CandidateStage,
        satisfied: bool,
        execution_context: Sha256Digest,
    ) -> Result<(Option<CandidateCheckpoint>, bool), ProductRunnerError> {
        self.record_observation(
            requested_stage,
            conversation_revision,
            CheckpointEvidence::Gates { satisfied, execution_context },
            Some(expected),
            ObservationOwner::Active,
        )
    }

    fn record_observation(
        &self,
        requested_stage: CandidateStage,
        conversation_revision: u64,
        acquired: CheckpointEvidence,
        expected: Option<CandidateIdentity>,
        owner: ObservationOwner,
    ) -> Result<(Option<CandidateCheckpoint>, bool), ProductRunnerError> {
        let mut recorder_state = self.lock()?;
        recorder_state.ensure_active()?;
        self.check_capture_cancelled_for(owner)?;
        let (has_workspace_candidate, content, repository) =
            self.capture_candidate_observation(owner)?;
        self.check_capture_cancelled_for(owner)?;
        if !has_workspace_candidate && !recorder_state.external_effect_observed {
            // A fresh repository observation is authoritative: a reverted workspace must not
            // retain an older candidate merely because it once contained changes.
            recorder_state.reducer = SettlementReducer::new();
            return Ok((None, expected.is_none()));
        }
        let mut replacement = *recorder_state;
        let expected_matches = expected.is_none_or(|expected| {
            candidate_material_matches(
                expected,
                self.run_id,
                self.workspace_id,
                content,
                repository,
                conversation_revision,
            )
        });
        let (requested_stage, acquired) = if expected_matches {
            (requested_stage, acquired)
        } else {
            (CandidateStage::Observed, CheckpointEvidence::None)
        };
        replacement.next_sequence =
            replacement.next_sequence.checked_add(1).ok_or_else(sequence_overflow)?;
        if let CheckpointEvidence::Gates { execution_context, .. } = acquired {
            replacement.execution_context = Some(execution_context);
        }
        let identity = CandidateIdentity::new(
            self.run_id,
            self.workspace_id,
            content,
            repository,
            replacement.execution_context,
            conversation_revision,
            replacement.next_sequence,
        )
        .map_err(invariant)?;
        let previous = replacement.reducer.checkpoint().copied();
        // Preserve the qualification ceiling only while content and public requirements agree.
        // Repository and execution changes are reconciled by each evidence record's dependencies.
        let contract_changed = acquired == CheckpointEvidence::ObligationContractChanged;
        let mut stage = previous.map_or(requested_stage, |checkpoint| {
            if checkpoint.identity().same_content_and_requirements(&identity) {
                stronger(checkpoint.stage(), requested_stage)
            } else {
                requested_stage
            }
        });
        let mut gates = previous.map_or(EvidenceStatus::Missing, |value| *value.gates());
        let mut obligations =
            previous.map_or(EvidenceStatus::Missing, |value| *value.obligations());
        let mut review = previous.map_or(EvidenceStatus::Missing, |value| *value.review());
        match acquired {
            CheckpointEvidence::None => {}
            CheckpointEvidence::Gates { satisfied, .. } => {
                gates = observed(identity, EvidenceDependencies::GATES, satisfied);
            }
            CheckpointEvidence::Obligations(satisfied) => {
                obligations = observed(identity, EvidenceDependencies::OBLIGATIONS, satisfied);
            }
            CheckpointEvidence::ObligationContractChanged => {
                obligations = EvidenceStatus::Missing;
                review = EvidenceStatus::Missing;
            }
            CheckpointEvidence::Review(satisfied) => {
                review = observed(identity, EvidenceDependencies::REVIEW, satisfied);
            }
            CheckpointEvidence::ExternalEffect => {
                obligations = EvidenceStatus::Missing;
                review = EvidenceStatus::Missing;
            }
        }
        if contract_changed {
            stage = if gates.is_current_and_satisfied(&identity) {
                CandidateStage::GatesPassed
            } else {
                CandidateStage::Observed
            };
        }
        let checkpoint = CandidateCheckpoint::observe(identity, stage, gates, obligations, review)
            .map_err(invariant)?;
        replacement.reducer.observe(checkpoint).map_err(invariant)?;
        *recorder_state = replacement;
        drop(recorder_state);
        Ok((Some(checkpoint), expected_matches))
    }

    pub(super) fn refresh(
        &self,
        conversation_revision: u64,
    ) -> Result<Option<CandidateCheckpoint>, ProductRunnerError> {
        self.record(CandidateStage::Observed, conversation_revision, CheckpointEvidence::None)
    }

    /// Refreshes after active work has stopped, retaining explicit caller cancellation while
    /// excluding the provider token already consumed by an active-work deadline.
    pub(super) fn refresh_for_finalization(
        &self,
        conversation_revision: u64,
    ) -> Result<Option<CandidateCheckpoint>, ProductRunnerError> {
        self.record_observation(
            CandidateStage::Observed,
            conversation_revision,
            CheckpointEvidence::None,
            None,
            ObservationOwner::Finalization,
        )
        .map(|(checkpoint, _)| checkpoint)
    }

    pub(super) fn record_pending_review(
        &self,
        conversation_revision: u64,
    ) -> Result<(), ProductRunnerError> {
        if self.checkpoint()?.is_some_and(|checkpoint| {
            checkpoint.review().is_current_and_satisfied(checkpoint.identity())
        }) {
            // A retry may reacquire gates for an unchanged candidate with a completed review.
            // Keep that valid evidence while the new review runs; it is not a pending review.
            return Ok(());
        }
        self.record(CandidateStage::ReviewPending, conversation_revision, CheckpointEvidence::None)
            .map(|_| ())
    }

    /// Adopts a newly published obligation representation without carrying qualification from
    /// the predecessor contract. Matching gate evidence remains usable; obligation and review
    /// conclusions must be reacquired against the new ledger root.
    pub(super) fn adopt_obligation_contract(
        &self,
        conversation_revision: u64,
    ) -> Result<(), ProductRunnerError> {
        self.record(
            CandidateStage::Observed,
            conversation_revision,
            CheckpointEvidence::ObligationContractChanged,
        )
        .map(|_| ())
    }

    pub(super) fn adopt_obligation_contract_for_finalization(
        &self,
        conversation_revision: u64,
    ) -> Result<(), ProductRunnerError> {
        self.record_observation(
            CandidateStage::Observed,
            conversation_revision,
            CheckpointEvidence::ObligationContractChanged,
            None,
            ObservationOwner::Finalization,
        )
        .map(|_| ())
    }

    #[cfg(test)]
    pub(super) fn settle(
        &self,
        cause: SettlementCause,
    ) -> Result<RunSettlement, ProductRunnerError> {
        let mut state = self.lock()?;
        let mut replacement = *state;
        let settlement = replacement.reducer.settle(cause).map_err(invariant)?;
        *state = replacement;
        Ok(settlement)
    }

    pub(super) fn checkpoint(&self) -> Result<Option<CandidateCheckpoint>, ProductRunnerError> {
        Ok(self.lock()?.reducer.checkpoint().copied())
    }

    /// Reads only the last atomically published, verified checkpoint for terminal recovery.
    /// Poison remains set: ordinary observation, admission, and effects still fail closed.
    pub(super) fn checkpoint_for_handoff(
        &self,
    ) -> (Option<CandidateCheckpoint>, Option<ProductRunnerError>) {
        match self.state.lock() {
            Ok(state) => (state.reducer.checkpoint().copied(), None),
            Err(poison) => {
                let state = poison.into_inner();
                (state.reducer.checkpoint().copied(), Some(poisoned_recorder()))
            }
        }
    }

    /// Consumes the same reducer's sole terminal transition even after a recorder failure.
    /// A poisoned recorder can produce only Recovery or explicit Cancellation, never Accepted.
    pub(super) fn settle_for_handoff(
        &self,
        cause: SettlementCause,
    ) -> Result<(RunSettlement, Option<ProductRunnerError>), ProductRunnerError> {
        let (mut state, failure) = match self.state.lock() {
            Ok(state) => (state, None),
            Err(poison) => (poison.into_inner(), Some(poisoned_recorder())),
        };
        let cause = if failure.is_some() && cause != SettlementCause::Cancellation {
            SettlementCause::Recovery
        } else {
            cause
        };
        let mut replacement = *state;
        let settlement = replacement.reducer.settle(cause).map_err(invariant)?;
        *state = replacement;
        Ok((settlement, failure))
    }

    fn mark_external_effect(&self) -> Result<(), ProductRunnerError> {
        let mut state = self.lock()?;
        state.ensure_active()?;
        let mut replacement = *state;
        replacement.external_effect_observed = true;
        *state = replacement;
        Ok(())
    }

    fn capture_candidate_axes(&self) -> Result<(Sha256Digest, Sha256Digest), ProductRunnerError> {
        crate::candidate::process::with_cancellation(
            self.observation_cancellation(),
            || self.capture_candidate_axes_inner(),
        )
    }

    fn capture_candidate_observation(
        &self,
        owner: ObservationOwner,
    ) -> Result<(bool, Sha256Digest, Sha256Digest), ProductRunnerError> {
        crate::candidate::process::with_cancellation(
            self.observation_cancellation_for(owner),
            || self.capture_candidate_observation_inner(owner),
        )
    }

    fn capture_candidate_observation_inner(
        &self,
        owner: ObservationOwner,
    ) -> Result<(bool, Sha256Digest, Sha256Digest), ProductRunnerError> {
        self.check_capture_cancelled_for(owner)?;
        if let Some(observation) = self.baseline.snapshot_observation(&self.root)? {
            self.check_capture_cancelled_for(owner)?;
            return Ok(observation.into_parts());
        }
        loop {
            self.check_capture_cancelled_for(owner)?;
            let before = self.baseline.checkpoint(&self.root)?.digest();
            self.check_capture_cancelled_for(owner)?;
            let has_workspace_candidate = !self.baseline.changed_paths(&self.root)?.is_empty();
            self.check_capture_cancelled_for(owner)?;
            let content = self.baseline.content_digest(&self.root)?;
            self.check_capture_cancelled_for(owner)?;
            let after = self.baseline.checkpoint(&self.root)?.digest();
            self.check_capture_cancelled_for(owner)?;
            if before == after {
                return Ok((has_workspace_candidate, content, after));
            }
            self.check_capture_cancelled_for(owner)?;
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn capture_candidate_axes_inner(
        &self,
    ) -> Result<(Sha256Digest, Sha256Digest), ProductRunnerError> {
        self.check_capture_cancelled()?;
        if let Some(observation) = self.baseline.snapshot_observation(&self.root)? {
            self.check_capture_cancelled()?;
            return Ok(observation.axes());
        }
        loop {
            self.check_capture_cancelled()?;
            let before = self.baseline.checkpoint(&self.root)?.digest();
            self.check_capture_cancelled()?;
            let content = self.baseline.content_digest(&self.root)?;
            self.check_capture_cancelled()?;
            let after = self.baseline.checkpoint(&self.root)?.digest();
            self.check_capture_cancelled()?;
            if before == after {
                return Ok((content, after));
            }
            // Contention changes no checkpoint, evidence, sequence, or run identity. This is a
            // retry cadence on the owned worker, not an attempt allowance or elapsed deadline.
            self.check_capture_cancelled()?;
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn observation_cancellation(&self) -> crate::candidate::process::Cancellation {
        self.observation_cancellation_for(ObservationOwner::Active)
    }

    fn observation_cancellation_for(
        &self,
        owner: ObservationOwner,
    ) -> crate::candidate::process::Cancellation {
        crate::candidate::process::Cancellation::new(
            Arc::clone(&self.cancelled),
            match owner {
                ObservationOwner::Active => self.provider_cancellation.clone(),
                ObservationOwner::Finalization => {
                    peritus_provider_core::CancellationToken::new()
                }
            },
        )
    }

    fn check_capture_cancelled(&self) -> Result<(), ProductRunnerError> {
        self.check_capture_cancelled_for(ObservationOwner::Active)
    }

    fn check_capture_cancelled_for(
        &self,
        owner: ObservationOwner,
    ) -> Result<(), ProductRunnerError> {
        if self.cancelled.load(Ordering::Acquire)
            || (matches!(owner, ObservationOwner::Active)
                && self.provider_cancellation.is_cancelled())
        {
            return Err(ProductRunnerError::new(
                ProductRunnerErrorKind::Cancelled,
                "observe candidate identity",
                "candidate consistency observation was cancelled; the previous checkpoint is retained",
            ));
        }
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, RecorderState>, ProductRunnerError> {
        self.state.lock().map_err(|_| poisoned_recorder())
    }
}

impl RecorderState {
    fn ensure_active(&self) -> Result<(), ProductRunnerError> {
        if self.reducer.terminal().is_some() {
            return Err(invariant("candidate recorder has already settled"));
        }
        Ok(())
    }
}

fn poisoned_recorder() -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InternalInvariant,
        "lock candidate checkpoint recorder",
        "candidate checkpoint recorder is poisoned; the last verified publication is retained for recovery only",
    )
}

const fn candidate_material_matches(
    expected: CandidateIdentity,
    run_id: RunId,
    workspace_id: WorkspaceId,
    content: Sha256Digest,
    repository: Sha256Digest,
    conversation_revision: u64,
) -> bool {
    expected.run_id() == run_id
        && expected.workspace_id() == workspace_id
        && expected.content_digest() == content
        && expected.repository_digest() == repository
        && expected.requirements_revision() == conversation_revision
}

const fn observed(
    identity: CandidateIdentity,
    dependencies: EvidenceDependencies,
    satisfied: bool,
) -> EvidenceStatus<QualificationEvidence> {
    let value = if satisfied {
        QualificationEvidence::Satisfied
    } else {
        QualificationEvidence::Unsatisfied
    };
    let record = EvidenceRecord::new(identity, dependencies, value);
    if satisfied { EvidenceStatus::Current(record) } else { EvidenceStatus::Failed(record) }
}

const fn stronger(left: CandidateStage, right: CandidateStage) -> CandidateStage {
    if left.rank() >= right.rank() { left } else { right }
}

fn invariant(error: impl std::fmt::Display) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InternalInvariant,
        "record candidate checkpoint",
        error.to_string(),
    )
}

fn sequence_overflow() -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InternalInvariant,
        "record candidate checkpoint",
        "candidate checkpoint sequence overflowed",
    )
}

#[cfg(test)]
mod tests;
