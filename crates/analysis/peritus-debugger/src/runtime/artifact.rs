//! Canonical validated-report artifact finalization.

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, Publication, WriteRequest,
};
use peritus_journal::SqliteJournal;
use peritus_types::{EventId, Sha256Digest};

use crate::{
    DebuggerCommand, DebuggerCommandKind, DebuggerError, DebuggerErrorKind, DebuggerOperation,
    DebuggerRecovery, DebuggerState, ReportId, ReportRecord, ValidatedReport,
    commit_debugger_transition, decide,
};

use super::{CommittedDebuggerTransition, TransitionIds};

/// Exact verified report artifact and distinct semantic payload digest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FinalizedReportArtifact {
    report_id: ReportId,
    payload_digest: Sha256Digest,
    artifact_digest: ArtifactDigest,
    size: u64,
    publication: Publication,
}

impl FinalizedReportArtifact {
    /// Report semantic identity.
    #[must_use]
    pub const fn report_id(self) -> ReportId {
        self.report_id
    }
    /// Domain-separated validated-report payload digest.
    #[must_use]
    pub const fn payload_digest(self) -> Sha256Digest {
        self.payload_digest
    }
    /// Raw SHA-256 artifact content digest.
    #[must_use]
    pub const fn artifact_digest(self) -> ArtifactDigest {
        self.artifact_digest
    }
    /// Exact canonical byte length.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }
    /// Whether bytes were new or already finalized.
    #[must_use]
    pub const fn publication(self) -> Publication {
        self.publication
    }
}

/// Produces the durable report record used by `CompleteReport`.
///
/// The retained digest is the raw artifact content digest so C0 artifact dependencies and later
/// publication state refer to the same bytes.
///
/// # Errors
/// Rejects an unrepresentable or empty canonical report.
pub fn report_record(report: &ValidatedReport) -> Result<ReportRecord, DebuggerError> {
    let size = u64::try_from(report.canonical_bytes().len())
        .map_err(|_| artifact_error("validated report size cannot be represented"))?;
    ReportRecord::new(report.id(), peritus_codec::sha256(report.canonical_bytes()), size)
}

/// Streams exact canonical report bytes to the C0 artifact owner and verifies finalization.
///
/// # Errors
/// Rejects size/digest drift or artifact-store failure. Exact retries observe existing bytes.
pub fn finalize_report_artifact(
    store: &ArtifactStore,
    report: &ValidatedReport,
    creating_event: EventId,
) -> Result<FinalizedReportArtifact, DebuggerError> {
    let record = report_record(report)?;
    let digest = ArtifactDigest::from_sha256(record.digest());
    let media_type =
        MediaType::new("application/vnd.peritus.debugger-report+json").map_err(artifact)?;
    let request = WriteRequest::new(
        digest,
        record.size(),
        record.size(),
        media_type,
        EncryptionMetadata::unencrypted(),
        creating_event,
    );
    let mut writer = store.begin_write(request).map_err(artifact)?;
    writer.write_chunk(report.canonical_bytes()).map_err(artifact)?;
    let finalized = writer.finalize().map_err(artifact)?;
    let metadata = store.verify(finalized.digest()).map_err(artifact)?;
    if finalized.digest() != digest
        || finalized.size() != record.size()
        || metadata.size() != record.size()
    {
        return Err(artifact_error(
            "finalized report artifact differs from validated canonical bytes",
        ));
    }
    Ok(FinalizedReportArtifact {
        report_id: report.id(),
        payload_digest: report.digest(),
        artifact_digest: finalized.digest(),
        size: finalized.size(),
        publication: finalized.publication(),
    })
}

/// Stages exact report bytes and commits the only transition that may reference them.
///
/// Retrying with the same report and identities reuses the content-addressed finalized artifact
/// and C0 command result. If C0 commit fails, the unreferenced finalized bytes remain harmless
/// staging and the same retry can reconcile them without creating another logical report.
///
/// # Errors
/// Rejects artifact finalization/verification drift or an invalid C0 transition.
pub fn stage_and_commit_report(
    journal: &mut SqliteJournal,
    artifact_store: &ArtifactStore,
    state: &DebuggerState,
    report: &ValidatedReport,
    ids: TransitionIds,
) -> Result<(FinalizedReportArtifact, CommittedDebuggerTransition), DebuggerError> {
    let staged = finalize_report_artifact(artifact_store, report, ids.event_id())?;
    let committed = commit_report_ready(journal, artifact_store, state, report, staged, ids)?;
    Ok((staged, committed))
}

/// Commits `CompleteReport` only after the exact staged content-addressed artifact is verified.
///
/// This gives C0 a referenceable `ArtifactDependency` at the report event position. Finalization
/// is staging, not logical publication; evidence admission and `Published` remain behind the
/// durable publication directive.
///
/// # Errors
/// Rejects report/artifact drift, missing staged content, stale state, or C0 commit failure.
pub fn commit_report_ready(
    journal: &mut SqliteJournal,
    artifact_store: &ArtifactStore,
    state: &DebuggerState,
    report: &ValidatedReport,
    staged: FinalizedReportArtifact,
    ids: TransitionIds,
) -> Result<CommittedDebuggerTransition, DebuggerError> {
    let record = report_record(report)?;
    if staged.report_id() != report.id()
        || staged.payload_digest() != report.digest()
        || staged.artifact_digest().sha256() != record.digest()
        || staged.size() != record.size()
    {
        return Err(artifact_error("staged artifact differs from the validated report"));
    }
    artifact_store.verify(staged.artifact_digest()).map_err(artifact)?;
    let command = DebuggerCommand::new(
        ids.command_id(),
        ids.event_id(),
        state.job_id(),
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.query_digest(),
        DebuggerCommandKind::CompleteReport { report: record },
    )?;
    let transition = decide(Some(state), &command)?;
    let batch = commit_debugger_transition(journal, &command, &transition)?;
    Ok(CommittedDebuggerTransition::new(batch, transition.state().clone()))
}

fn artifact(error: impl core::fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Artifact,
        DebuggerOperation::PublishArtifact,
        DebuggerRecovery::Reconcile,
        error.to_string(),
    )
}
fn artifact_error(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Artifact,
        DebuggerOperation::PublishArtifact,
        DebuggerRecovery::Reconcile,
        detail,
    )
}
