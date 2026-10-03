//! Exact candidate observation at every material product-run boundary.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceDependencies, EvidenceRecord,
    EvidenceStatus, QualificationEvidence, RunSettlement, SettlementCause, SettlementReducer,
};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

use super::ConversationView;
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
    state: Arc<Mutex<RecorderState>>,
}

#[derive(Clone, Copy)]
struct RecorderState {
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
    Review(bool),
    ExternalEffect,
}

impl CandidateRecorder {
    pub(super) fn new(
        root: &Path,
        baseline: CandidateBaseline,
        run_id: RunId,
        workspace_id: WorkspaceId,
        prior: Option<&CandidateCheckpoint>,
        retain_external_effect: bool,
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
            root: root.to_owned(),
            baseline,
            run_id,
            workspace_id,
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
        let has_workspace_candidate = !self.baseline.changed_paths(&self.root)?.is_empty();
        let mut recorder_state = self.lock()?;
        if !has_workspace_candidate && !recorder_state.external_effect_observed {
            // A fresh repository observation is authoritative: a reverted workspace must not
            // retain an older candidate merely because it once contained changes.
            recorder_state.reducer = SettlementReducer::new();
            drop(recorder_state);
            return Ok(None);
        }
        drop(recorder_state);
        let (content, repository) = self.capture_candidate_axes()?;
        let mut recorder_state = self.lock()?;
        recorder_state.next_sequence =
            recorder_state.next_sequence.checked_add(1).ok_or_else(sequence_overflow)?;
        if let CheckpointEvidence::Gates { execution_context, .. } = acquired {
            recorder_state.execution_context = Some(execution_context);
        }
        let identity = CandidateIdentity::new(
            self.run_id,
            self.workspace_id,
            content,
            repository,
            recorder_state.execution_context,
            conversation_revision,
            recorder_state.next_sequence,
        )
        .map_err(invariant)?;
        let previous = recorder_state.reducer.checkpoint().copied();
        // Preserve the qualification ceiling only while content and public requirements agree.
        // Repository and execution changes are reconciled by each evidence record's dependencies.
        let stage = previous.map_or(requested_stage, |checkpoint| {
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
            CheckpointEvidence::Review(satisfied) => {
                review = observed(identity, EvidenceDependencies::REVIEW, satisfied);
            }
            CheckpointEvidence::ExternalEffect => {
                obligations = EvidenceStatus::Missing;
                review = EvidenceStatus::Missing;
            }
        }
        let checkpoint = CandidateCheckpoint::observe(identity, stage, gates, obligations, review)
            .map_err(invariant)?;
        recorder_state.reducer.observe(checkpoint).map_err(invariant)?;
        drop(recorder_state);
        Ok(Some(checkpoint))
    }

    pub(super) fn refresh(
        &self,
        conversation_revision: u64,
    ) -> Result<Option<CandidateCheckpoint>, ProductRunnerError> {
        self.record(CandidateStage::Observed, conversation_revision, CheckpointEvidence::None)
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

    pub(super) fn settle(
        &self,
        cause: SettlementCause,
    ) -> Result<RunSettlement, ProductRunnerError> {
        self.lock()?.reducer.settle(cause).map_err(invariant)
    }

    pub(super) fn checkpoint(&self) -> Result<Option<CandidateCheckpoint>, ProductRunnerError> {
        Ok(self.lock()?.reducer.checkpoint().copied())
    }

    fn mark_external_effect(&self) -> Result<(), ProductRunnerError> {
        self.lock()?.external_effect_observed = true;
        Ok(())
    }

    fn capture_candidate_axes(&self) -> Result<(Sha256Digest, Sha256Digest), ProductRunnerError> {
        for _ in 0..3 {
            let before = self.baseline.checkpoint(&self.root)?.digest();
            let content = self.baseline.content_digest(&self.root)?;
            let after = self.baseline.checkpoint(&self.root)?.digest();
            if before == after {
                return Ok((content, after));
            }
        }
        Err(ProductRunnerError::new(
            ProductRunnerErrorKind::Repository,
            "observe candidate identity",
            "workspace changed repeatedly while capturing content and repository context",
        ))
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, RecorderState>, ProductRunnerError> {
        self.state.lock().map_err(|_| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::InternalInvariant,
                "lock candidate checkpoint recorder",
                "candidate checkpoint recorder is poisoned",
            )
        })
    }
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
