//! Provenance-checked evidence admission and publication settlement.

use peritus_artifact_store::ArtifactStore;
use peritus_evidence::{
    EvidenceDraft, EvidenceKind, EvidenceRecord, EvidenceSource, EvidenceStore,
};
use peritus_journal::SqliteJournal;
use peritus_types::EvidenceId;

use crate::{
    EvaluationCommand, EvaluationCommandKind, EvaluationError, EvaluationErrorKind,
    EvaluationOperation, EvaluationPhase, EvaluationRecovery, EvaluationState,
    PublicationDirectiveClaim, PublicationRecord, ValidatedEvaluationReport,
    commit_evaluation_settlement, decide,
};

use super::{CommittedEvaluationTransition, FinalizedEvaluationArtifact, TransitionIds};

/// Admitted evidence plus exact atomic C0 publication settlement.
#[derive(Debug)]
pub struct PublicationExecution {
    evidence: EvidenceRecord,
    committed: CommittedEvaluationTransition,
}

impl PublicationExecution {
    /// Immutable admitted evaluation-report evidence.
    #[must_use]
    pub const fn evidence(&self) -> &EvidenceRecord {
        &self.evidence
    }
    /// Publication event/checkpoint/outbox acknowledgement commit.
    #[must_use]
    pub const fn committed(&self) -> &CommittedEvaluationTransition {
        &self.committed
    }
    /// Consumes the complete publication result.
    #[must_use]
    pub fn into_parts(self) -> (EvidenceRecord, CommittedEvaluationTransition) {
        (self.evidence, self.committed)
    }
}

/// Admits exact report evidence and settles its claimed publication directive.
///
/// # Errors
/// Rejects binding drift, artifact/evidence failure, invalid provenance, or stale C0 claims.
#[allow(clippy::too_many_arguments, reason = "publication owners and exact fences stay explicit")]
pub fn publish_claimed_report(
    journal: &mut SqliteJournal,
    evidence_store: &mut EvidenceStore,
    artifact_store: &ArtifactStore,
    state: &EvaluationState,
    report: &ValidatedEvaluationReport,
    artifact: FinalizedEvaluationArtifact,
    report_commit_position: u64,
    claim: PublicationDirectiveClaim,
    ids: TransitionIds,
) -> Result<PublicationExecution, EvaluationError> {
    let durable_report =
        state.report().ok_or_else(|| binding("report-ready state has no report"))?;
    let directive = *claim.directive();
    if state.phase() != EvaluationPhase::ReportReady
        || durable_report.id() != report.id()
        || durable_report.artifact() != artifact.artifact_digest()
        || durable_report.size() != artifact.size()
        || artifact.payload_digest() != report.digest()
        || directive.campaign_id() != state.campaign_id()
        || directive.report() != durable_report
        || report_commit_position == 0
    {
        return Err(binding("publication state, claim, report, or artifact differs"));
    }
    artifact_store.verify(artifact.artifact_digest()).map_err(artifact_error)?;
    let evidence_id = report_evidence_id(report)?;
    let draft = EvidenceDraft::new(
        evidence_id,
        EvidenceKind::new("evaluation-report").map_err(evidence_error)?,
        EvidenceSource::new("peritus-eval").map_err(evidence_error)?,
        *state.revision(),
        report_commit_position,
        report.digest(),
        vec![artifact.artifact_digest()],
        Vec::new(),
    )
    .map_err(evidence_error)?;
    let export = journal.integrity_export().map_err(journal_error)?;
    let evidence = evidence_store.admit(draft, &export, artifact_store).map_err(evidence_error)?;
    let publication = PublicationRecord::new(report.id(), evidence.id(), report_commit_position)?;
    let command = EvaluationCommand::new(
        ids.command_id(),
        ids.event_id(),
        state.campaign_id(),
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.profile_digest(),
        EvaluationCommandKind::RecordPublication { publication },
    )?;
    let transition = decide(Some(state), &command)?;
    let batch = commit_evaluation_settlement(journal, &command, &transition, claim)?;
    Ok(PublicationExecution {
        evidence,
        committed: CommittedEvaluationTransition::new(batch, transition.state().clone()),
    })
}

fn report_evidence_id(report: &ValidatedEvaluationReport) -> Result<EvidenceId, EvaluationError> {
    let mut bytes = b"peritus.evaluation.report-evidence.v1\0".to_vec();
    bytes.extend_from_slice(report.id().as_bytes());
    bytes.extend_from_slice(report.digest().as_bytes());
    let digest = peritus_codec::sha256(&bytes);
    let mut id = [0; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    id[0] |= 0x40;
    EvidenceId::new(id).map_err(|_| binding("derived report evidence identity is invalid"))
}
const fn binding(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Binding,
        EvaluationOperation::Publish,
        EvaluationRecovery::Quarantine,
        detail,
    )
}
fn artifact_error(_: impl core::fmt::Display) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Artifact,
        EvaluationOperation::Publish,
        EvaluationRecovery::Reconcile,
        "report artifact verification failed",
    )
}
fn evidence_error(_: impl core::fmt::Display) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Evidence,
        EvaluationOperation::Publish,
        EvaluationRecovery::Reconcile,
        "evaluation evidence admission failed",
    )
}
fn journal_error(_: impl core::fmt::Display) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Journal,
        EvaluationOperation::Publish,
        EvaluationRecovery::Replay,
        "journal integrity export failed",
    )
}
