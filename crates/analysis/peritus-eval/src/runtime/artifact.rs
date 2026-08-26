//! Canonical evaluation-report artifact finalization and commit.

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, Publication, WriteRequest,
};
use peritus_journal::SqliteJournal;
use peritus_types::{EventId, Sha256Digest};

use crate::{
    EvaluationCommand, EvaluationCommandKind, EvaluationError, EvaluationErrorKind,
    EvaluationOperation, EvaluationPhase, EvaluationRecovery, EvaluationState, ReportRecord,
    ValidatedEvaluationReport, commit_evaluation_transition, decide,
};

use super::{CommittedEvaluationTransition, TransitionIds};

/// Exact verified report artifact and distinct semantic payload digest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FinalizedEvaluationArtifact {
    report_id: crate::EvaluationReportId,
    payload_digest: Sha256Digest,
    artifact_digest: ArtifactDigest,
    size: u64,
    publication: Publication,
}

impl FinalizedEvaluationArtifact {
    /// Report semantic identity.
    #[must_use]
    pub const fn report_id(self) -> crate::EvaluationReportId {
        self.report_id
    }
    /// Domain-separated semantic report digest.
    #[must_use]
    pub const fn payload_digest(self) -> Sha256Digest {
        self.payload_digest
    }
    /// Raw SHA-256 artifact identity.
    #[must_use]
    pub const fn artifact_digest(self) -> ArtifactDigest {
        self.artifact_digest
    }
    /// Exact canonical byte length.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }
    /// Whether finalization created or reused content.
    #[must_use]
    pub const fn publication(self) -> Publication {
        self.publication
    }
}

/// Streams exact report bytes to the artifact owner and verifies finalization.
///
/// # Errors
/// Rejects empty/oversized bytes or any artifact staging, finalization, or verification failure.
pub fn finalize_report_artifact(
    store: &ArtifactStore,
    report: &ValidatedEvaluationReport,
    creating_event: EventId,
) -> Result<FinalizedEvaluationArtifact, EvaluationError> {
    let size =
        u64::try_from(report.bytes().len()).map_err(|_| artifact("report size overflowed"))?;
    if size == 0 {
        return Err(artifact("canonical report is empty"));
    }
    let artifact_digest = ArtifactDigest::from_sha256(peritus_codec::sha256(report.bytes()));
    let media_type = MediaType::new("application/vnd.peritus.evaluation-report+binary")
        .map_err(|_| artifact("evaluation report media type is invalid"))?;
    let request = WriteRequest::new(
        artifact_digest,
        size,
        size,
        media_type,
        EncryptionMetadata::unencrypted(),
        creating_event,
    );
    let mut writer = store.begin_write(request).map_err(artifact_owner)?;
    writer.write_chunk(report.bytes()).map_err(artifact_owner)?;
    let finalized = writer.finalize().map_err(artifact_owner)?;
    let metadata = store.verify(finalized.digest()).map_err(artifact_owner)?;
    if finalized.digest() != artifact_digest || finalized.size() != size || metadata.size() != size
    {
        return Err(artifact("finalized report differs from validated canonical bytes"));
    }
    Ok(FinalizedEvaluationArtifact {
        report_id: report.id(),
        payload_digest: report.digest(),
        artifact_digest,
        size,
        publication: finalized.publication(),
    })
}

/// Stages exact report bytes and commits the only event that references them.
///
/// # Errors
/// Rejects artifact failures, report/state drift, or a failed C0 transition.
pub fn stage_and_commit_report(
    journal: &mut SqliteJournal,
    artifact_store: &ArtifactStore,
    state: &EvaluationState,
    report: &ValidatedEvaluationReport,
    ids: TransitionIds,
) -> Result<(FinalizedEvaluationArtifact, CommittedEvaluationTransition), EvaluationError> {
    let staged = finalize_report_artifact(artifact_store, report, ids.event_id())?;
    let committed = commit_report_ready(journal, artifact_store, state, report, staged, ids)?;
    Ok((staged, committed))
}

/// Verifies staged bytes and atomically commits `CompleteReport` plus publication directive.
///
/// # Errors
/// Rejects mismatched state/report/artifact bindings or a failed artifact/C0 operation.
pub fn commit_report_ready(
    journal: &mut SqliteJournal,
    artifact_store: &ArtifactStore,
    state: &EvaluationState,
    report: &ValidatedEvaluationReport,
    staged: FinalizedEvaluationArtifact,
    ids: TransitionIds,
) -> Result<CommittedEvaluationTransition, EvaluationError> {
    if state.phase() != EvaluationPhase::Analyzing
        || state.analysis_digest().is_none()
        || report.report().campaign_id() != state.campaign_id()
        || report.report().profile_digest() != state.profile_digest()
        || staged.report_id() != report.id()
        || staged.payload_digest() != report.digest()
        || staged.artifact_digest().sha256() != peritus_codec::sha256(report.bytes())
        || staged.size()
            != u64::try_from(report.bytes().len())
                .map_err(|_| artifact("report size overflowed"))?
    {
        return Err(binding("state, report, or staged artifact binding differs"));
    }
    artifact_store.verify(staged.artifact_digest()).map_err(artifact_owner)?;
    let record =
        ReportRecord::new(report.id(), report.digest(), staged.artifact_digest(), staged.size())?;
    let command = EvaluationCommand::new(
        ids.command_id(),
        ids.event_id(),
        state.campaign_id(),
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.profile_digest(),
        EvaluationCommandKind::CompleteReport { report: record },
    )?;
    let transition = decide(Some(state), &command)?;
    let batch = commit_evaluation_transition(journal, &command, &transition)?;
    Ok(CommittedEvaluationTransition::new(batch, transition.state().clone()))
}

fn artifact_owner(_: impl core::fmt::Display) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Artifact,
        EvaluationOperation::Publish,
        EvaluationRecovery::Reconcile,
        "artifact owner failed to finalize or verify evaluation report",
    )
}
const fn artifact(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Artifact,
        EvaluationOperation::Publish,
        EvaluationRecovery::Reconcile,
        detail,
    )
}
const fn binding(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Binding,
        EvaluationOperation::Publish,
        EvaluationRecovery::Quarantine,
        detail,
    )
}
