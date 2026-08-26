//! Provenance-checked evidence admission and fenced publication settlement.

use peritus_artifact_store::ArtifactStore;
use peritus_evidence::{
    EvidenceDraft, EvidenceKind, EvidenceRecord, EvidenceSource, EvidenceStore,
};
use peritus_journal::SqliteJournal;
use peritus_types::EvidenceId;

use crate::{
    DebuggerCommand, DebuggerCommandKind, DebuggerError, DebuggerErrorKind, DebuggerOperation,
    DebuggerPhase, DebuggerRecovery, DebuggerState, PublicationDirectiveClaim, PublicationRecord,
    ValidatedReport, commit_debugger_settlement, decide,
};

use super::{CommittedDebuggerTransition, FinalizedReportArtifact, TransitionIds};

#[cfg(test)]
mod tests;

/// Evidence admission plus the exact atomic C0 publication settlement.
#[derive(Debug)]
pub struct PublicationExecution {
    evidence: EvidenceRecord,
    committed: CommittedDebuggerTransition,
}

impl PublicationExecution {
    /// Immutable admitted debugger-report evidence.
    #[must_use]
    pub const fn evidence(&self) -> &EvidenceRecord {
        &self.evidence
    }
    /// Publication event/checkpoint/outbox acknowledgement commit.
    #[must_use]
    pub const fn committed(&self) -> &CommittedDebuggerTransition {
        &self.committed
    }
    /// Consumes the complete publication result.
    #[must_use]
    pub fn into_parts(self) -> (EvidenceRecord, CommittedDebuggerTransition) {
        (self.evidence, self.committed)
    }
}

/// Admits evidence for an exact staged report and settles its claimed publication directive.
///
/// The report artifact must already be finalized and referenced by the `CompleteReport` event at
/// `report_commit_position`. Evidence identity is content-derived, so a crash after admission but
/// before C0 settlement is reconciled by the same exact retry.
///
/// # Errors
/// Rejects state/report/claim/artifact drift, invalid journal provenance, evidence conflict, or a
/// stale publication settlement.
#[allow(clippy::too_many_arguments, reason = "publication owners and exact fences stay explicit")]
pub fn publish_claimed_report(
    journal: &mut SqliteJournal,
    evidence_store: &mut EvidenceStore,
    artifact_store: &ArtifactStore,
    state: &DebuggerState,
    report: &ValidatedReport,
    artifact: FinalizedReportArtifact,
    report_commit_position: u64,
    claim: PublicationDirectiveClaim,
    ids: TransitionIds,
) -> Result<PublicationExecution, DebuggerError> {
    let durable_report =
        state.report().ok_or_else(|| binding("report-ready state has no durable report"))?;
    let directive = claim.directive();
    if state.phase() != DebuggerPhase::ReportReady
        || durable_report.id() != report.id()
        || durable_report.digest() != artifact.artifact_digest().sha256()
        || durable_report.size() != artifact.size()
        || artifact.report_id() != report.id()
        || artifact.payload_digest() != report.digest()
        || directive.job_id() != state.job_id()
        || directive.report() != durable_report
        || report_commit_position == 0
    {
        return Err(binding(
            "publication state, claim, artifact, report, or journal position differs",
        ));
    }
    artifact_store.verify(artifact.artifact_digest()).map_err(artifact_error)?;
    let evidence_id = report_evidence_id(report)?;
    let causes = report.report().supersedes().into_iter().collect::<Vec<_>>();
    let draft = EvidenceDraft::new(
        evidence_id,
        EvidenceKind::new("debugger-report").map_err(evidence_error)?,
        EvidenceSource::new("peritus-debugger").map_err(evidence_error)?,
        *state.revision(),
        report_commit_position,
        report.digest(),
        vec![artifact.artifact_digest()],
        causes,
    )
    .map_err(evidence_error)?;
    let export = journal.integrity_export().map_err(journal_error)?;
    let evidence = evidence_store.admit(draft, &export, artifact_store).map_err(evidence_error)?;
    let publication = PublicationRecord::new(
        report.id(),
        artifact.artifact_digest().sha256(),
        artifact.size(),
        evidence.id(),
        report_commit_position,
    )?;
    let command = DebuggerCommand::new(
        ids.command_id(),
        ids.event_id(),
        state.job_id(),
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.query_digest(),
        DebuggerCommandKind::RecordPublication { publication },
    )?;
    let transition = decide(Some(state), &command)?;
    let batch = commit_debugger_settlement(journal, &command, &transition, claim)?;
    Ok(PublicationExecution {
        evidence,
        committed: CommittedDebuggerTransition::new(batch, transition.state().clone()),
    })
}

fn report_evidence_id(report: &ValidatedReport) -> Result<EvidenceId, DebuggerError> {
    let mut bytes = b"peritus.debugger.report-evidence.v1\0".to_vec();
    bytes.extend_from_slice(report.id().as_bytes());
    bytes.extend_from_slice(report.digest().as_bytes());
    let digest = peritus_codec::sha256(&bytes);
    let mut id = [0_u8; 16];
    id.copy_from_slice(&digest.as_bytes()[..16]);
    EvidenceId::new(id).map_err(|_| binding("derived report evidence identity is invalid"))
}

fn binding(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Binding,
        DebuggerOperation::PublishEvidence,
        DebuggerRecovery::Quarantine,
        detail,
    )
}
fn artifact_error(error: impl core::fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Artifact,
        DebuggerOperation::PublishArtifact,
        DebuggerRecovery::Reconcile,
        error.to_string(),
    )
}
fn evidence_error(error: impl core::fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Evidence,
        DebuggerOperation::PublishEvidence,
        DebuggerRecovery::Reconcile,
        error.to_string(),
    )
}
fn journal_error(error: impl core::fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Journal,
        DebuggerOperation::PublishEvidence,
        DebuggerRecovery::ReplayAggregate,
        error.to_string(),
    )
}
