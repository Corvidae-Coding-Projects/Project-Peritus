//! Exact candidate observation at every material product-run boundary.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceRecord, EvidenceStatus,
    QualificationEvidence, RunSettlement, SettlementCause, SettlementReducer,
};
use peritus_types::{RunId, WorkspaceId};

use super::ConversationView;
use crate::{
    ProductRunnerError, ProductRunnerErrorKind, candidate::CandidateBaseline,
    developer_tools::ToolCheckpointBoundary, progress::WorkspaceCheckpoint,
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
}

/// Evidence acquired at the candidate boundary being recorded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CheckpointEvidence {
    None,
    Gates(bool),
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
            })),
        })
    }

    pub(crate) fn tool_observer(
        &self,
        conversation: Arc<dyn ConversationView>,
    ) -> Arc<dyn Fn(ToolCheckpointBoundary) -> Result<(), String> + Send + Sync> {
        let recorder = self.clone();
        Arc::new(move |boundary| {
            let revision = conversation.revision();
            match boundary {
                ToolCheckpointBoundary::Mutation => {
                    recorder.record(CandidateStage::Changed, revision, CheckpointEvidence::None)
                }
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
        let workspace = WorkspaceCheckpoint::capture(&self.root)?;
        let has_workspace_candidate = !self.baseline.changed_paths(&self.root)?.is_empty();
        let mut recorder_state = self.lock()?;
        if !has_workspace_candidate && !recorder_state.external_effect_observed {
            // A fresh repository observation is authoritative: a reverted workspace must not
            // retain an older candidate merely because it once contained changes.
            recorder_state.reducer = SettlementReducer::new();
            return Ok(None);
        }
        recorder_state.next_sequence =
            recorder_state.next_sequence.checked_add(1).ok_or_else(sequence_overflow)?;
        let identity = CandidateIdentity::new(
            self.run_id,
            self.workspace_id,
            workspace.digest(),
            conversation_revision,
            recorder_state.next_sequence,
        )
        .map_err(invariant)?;
        let previous = recorder_state.reducer.checkpoint().copied();
        let same_candidate = previous
            .as_ref()
            .is_some_and(|checkpoint| checkpoint.identity().same_candidate(&identity));
        let stage = previous
            .filter(|_| same_candidate)
            .map_or(requested_stage, |checkpoint| stronger(checkpoint.stage(), requested_stage));
        let mut gates = carry(previous.as_ref().map(CandidateCheckpoint::gates), same_candidate);
        let mut obligations =
            carry(previous.as_ref().map(CandidateCheckpoint::obligations), same_candidate);
        let mut review = carry(previous.as_ref().map(CandidateCheckpoint::review), same_candidate);
        match acquired {
            CheckpointEvidence::None => {}
            CheckpointEvidence::Gates(satisfied) => {
                gates = observed(identity, satisfied);
            }
            CheckpointEvidence::Obligations(satisfied) => {
                obligations = observed(identity, satisfied);
            }
            CheckpointEvidence::Review(satisfied) => {
                review = observed(identity, satisfied);
            }
            CheckpointEvidence::ExternalEffect => {
                obligations = EvidenceStatus::Missing;
                review = EvidenceStatus::Missing;
            }
        }
        let checkpoint = CandidateCheckpoint::new(identity, stage, gates, obligations, review)
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

const fn carry(
    status: Option<&EvidenceStatus<QualificationEvidence>>,
    same_candidate: bool,
) -> EvidenceStatus<QualificationEvidence> {
    match status.copied() {
        None | Some(EvidenceStatus::Missing) => EvidenceStatus::Missing,
        Some(EvidenceStatus::Current(record) | EvidenceStatus::Failed(record))
            if same_candidate =>
        {
            if record.value().satisfied() {
                EvidenceStatus::Current(record)
            } else {
                EvidenceStatus::Failed(record)
            }
        }
        Some(EvidenceStatus::Current(record) | EvidenceStatus::Failed(record)) => {
            EvidenceStatus::Stale(record)
        }
        Some(EvidenceStatus::Stale(record)) => EvidenceStatus::Stale(record),
    }
}

const fn observed(
    identity: CandidateIdentity,
    satisfied: bool,
) -> EvidenceStatus<QualificationEvidence> {
    let value = if satisfied {
        QualificationEvidence::Satisfied
    } else {
        QualificationEvidence::Unsatisfied
    };
    let record = EvidenceRecord::new(identity, value);
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
mod tests {
    use std::{fs, process::Command};

    use super::*;

    #[test]
    fn changed_candidate_stales_prior_gate_evidence() {
        let root = repository();
        let baseline = CandidateBaseline::capture(root.path()).expect("baseline");
        let recorder =
            CandidateRecorder::new(root.path(), baseline, run_id(), workspace_id(), None, false)
                .expect("recorder");
        fs::write(root.path().join("candidate.txt"), "first").expect("first mutation");
        recorder
            .record(CandidateStage::GatesPassed, 1, CheckpointEvidence::Gates(true))
            .expect("gates");
        fs::write(root.path().join("candidate.txt"), "second").expect("second mutation");

        let checkpoint = recorder
            .record(CandidateStage::Changed, 1, CheckpointEvidence::None)
            .expect("mutation")
            .expect("candidate");

        assert!(matches!(checkpoint.gates(), EvidenceStatus::Stale(_)));
        assert_eq!(checkpoint.stage(), CandidateStage::Changed);
    }

    #[test]
    fn conversation_revision_stales_prior_evidence() {
        let root = repository();
        let baseline = CandidateBaseline::capture(root.path()).expect("baseline");
        let recorder =
            CandidateRecorder::new(root.path(), baseline, run_id(), workspace_id(), None, false)
                .expect("recorder");
        fs::write(root.path().join("candidate.txt"), "changed").expect("mutation");
        recorder
            .record(CandidateStage::GatesPassed, 1, CheckpointEvidence::Gates(true))
            .expect("gates");

        let checkpoint = recorder.refresh(2).expect("refresh").expect("candidate");

        assert!(matches!(checkpoint.gates(), EvidenceStatus::Stale(_)));
        assert_eq!(checkpoint.identity().conversation_revision(), 2);
    }

    #[test]
    fn fresh_reversion_clears_an_old_workspace_candidate() {
        let root = repository();
        let baseline = CandidateBaseline::capture(root.path()).expect("baseline");
        let recorder =
            CandidateRecorder::new(root.path(), baseline, run_id(), workspace_id(), None, false)
                .expect("recorder");
        fs::write(root.path().join("candidate.txt"), "changed").expect("mutation");
        assert!(recorder.refresh(1).expect("changed refresh").is_some());

        fs::write(root.path().join("candidate.txt"), "baseline").expect("reversion");

        assert!(recorder.refresh(1).expect("reverted refresh").is_none());
        assert!(recorder.checkpoint().expect("checkpoint").is_none());
    }

    fn repository() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("root");
        run(root.path(), &["init", "--quiet"]);
        run(root.path(), &["config", "user.email", "peritus@example.invalid"]);
        run(root.path(), &["config", "user.name", "Peritus Test"]);
        fs::write(root.path().join("candidate.txt"), "baseline").expect("baseline file");
        run(root.path(), &["add", "."]);
        run(root.path(), &["commit", "--quiet", "-m", "fixture"]);
        root
    }

    fn run(root: &Path, arguments: &[&str]) {
        assert!(Command::new("git").args(arguments).current_dir(root).status().unwrap().success());
    }

    fn run_id() -> RunId {
        RunId::new([1; 16]).expect("run id")
    }

    fn workspace_id() -> WorkspaceId {
        WorkspaceId::new([2; 16]).expect("workspace id")
    }
}
