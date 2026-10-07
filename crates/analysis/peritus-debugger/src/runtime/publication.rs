//! Provenance-checked evidence admission and fenced publication settlement.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore, ErrorCode, RecoveryClass};
use peritus_evidence::{
    EvidenceDraft, EvidenceError, EvidenceKind, EvidenceRecord, EvidenceSource, EvidenceStore,
    RecoveryAction as EvidenceRecoveryAction,
};
use peritus_journal::SqliteJournal;
use peritus_types::EvidenceId;

use crate::{
    DebuggerCommand, DebuggerCommandKind, DebuggerCommitMode, DebuggerError, DebuggerErrorKind,
    DebuggerEventKind, DebuggerOperation, DebuggerPhase, DebuggerRecovery, DebuggerState,
    PublicationDirectiveClaim, PublicationRecord, ValidatedReport, commit_debugger_settlement,
    debugger_aggregate_key, decide, load_debugger_operation,
};

use super::{
    CommittedDebuggerTransition, FinalizedReportArtifact, PublicationDependencyStatus,
    PublicationRecoveryObservation, TransitionIds, finalize_report_artifact, report_record,
    validate_recovered_claim, verify_report_artifact,
};

#[cfg(test)]
mod tests;

/// Evidence admission plus the exact atomic C0 publication settlement.
#[derive(Debug)]
pub struct PublicationExecution {
    evidence: EvidenceRecord,
    committed: CommittedDebuggerTransition,
}

/// Exact restored dependencies paired with the already accepted publication operation.
#[derive(Debug)]
pub struct CompletedPublicationRepair {
    artifact: FinalizedReportArtifact,
    evidence: EvidenceRecord,
    publication: CommittedDebuggerTransition,
}

impl CompletedPublicationRepair {
    /// Restored and verified canonical report artifact.
    #[must_use]
    pub const fn artifact(&self) -> FinalizedReportArtifact {
        self.artifact
    }
    /// Restored and verified immutable evidence record.
    #[must_use]
    pub const fn evidence(&self) -> &EvidenceRecord {
        &self.evidence
    }
    /// Original accepted publication with separately reconstructed current state.
    #[must_use]
    pub const fn publication(&self) -> &CommittedDebuggerTransition {
        &self.publication
    }
    /// Consumes the complete dependency-repair result.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (FinalizedReportArtifact, EvidenceRecord, CommittedDebuggerTransition) {
        (self.artifact, self.evidence, self.publication)
    }
}

/// Observes the exact artifact and evidence owners for one report without changing either store.
///
/// Authoritative absence and temporary owner unavailability are distinct. Malformed, quarantined,
/// or identity-divergent dependencies return an explicit integrity error instead of looking absent.
///
/// # Errors
/// Rejects report/publication binding drift or an unrecoverable owner integrity failure.
pub fn observe_publication_dependencies(
    artifact_store: &ArtifactStore,
    evidence_store: &EvidenceStore,
    state: &DebuggerState,
    report: &ValidatedReport,
    report_commit_position: u64,
) -> Result<PublicationRecoveryObservation, DebuggerError> {
    let (durable_report, draft, publication) =
        publication_plan(state, report, report_commit_position)?;
    if state.phase() == DebuggerPhase::Published
        && state.publication() != Some(publication)
    {
        return Err(recovery(
            "published state differs from the exact dependency observation",
        ));
    }
    let artifact = observe_artifact(artifact_store, durable_report, report)?;
    let evidence = match evidence_store.load(draft.id()) {
        Ok(Some(record)) => {
            validate_recovered_evidence(&record, &draft)?;
            PublicationDependencyStatus::Verified
        }
        Ok(None) => PublicationDependencyStatus::Missing,
        Err(error) if error.recovery() == EvidenceRecoveryAction::Retry => {
            PublicationDependencyStatus::Unavailable
        }
        Err(error) => return Err(evidence_integrity(error)),
    };
    Ok(PublicationRecoveryObservation::new(
        state.job_id(),
        durable_report,
        draft.id(),
        artifact,
        evidence,
    ))
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
    let (durable_report, draft, publication) =
        publication_plan(state, report, report_commit_position)?;
    let directive = claim.directive();
    if artifact.report_id() != report.id()
        || artifact.payload_digest() != report.digest()
        || artifact.artifact_digest().sha256() != durable_report.digest()
        || artifact.size() != durable_report.size()
        || directive.job_id() != state.job_id()
        || directive.report() != durable_report
    {
        return Err(binding(
            "publication state, claim, artifact, report, or journal position differs",
        ));
    }
    verify_report_artifact(artifact_store, report, artifact)?;
    if let Some(operation) = recover_claimed_publication(
        journal,
        state.job_id(),
        durable_report,
        publication,
        claim,
        ids,
    )? {
        let evidence = evidence_store
            .load(publication.evidence_id())
            .map_err(evidence_error)?
            .ok_or_else(|| recovery("recovered publication evidence is absent"))?;
        validate_recovered_evidence(&evidence, &draft)?;
        return Ok(PublicationExecution {
            evidence,
            committed: CommittedDebuggerTransition::new(operation),
        });
    }
    if state.phase() != DebuggerPhase::ReportReady {
        return Err(binding("publication state is not report-ready"));
    }
    let export = journal.integrity_export().map_err(journal_error)?;
    let evidence = evidence_store.admit(draft, &export, artifact_store).map_err(evidence_error)?;
    if evidence.id() != publication.evidence_id() {
        return Err(recovery("admitted evidence identity differs from publication"));
    }
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
    let operation = commit_debugger_settlement(journal, &command, &transition, claim)?;
    Ok(PublicationExecution {
        evidence,
        committed: CommittedDebuggerTransition::new(operation),
    })
}

/// Settles an interrupted publication from already admitted exact evidence.
///
/// This operation never admits or replaces evidence. It either recovers the accepted settlement
/// through its retained receipt or atomically settles the still-`ReportReady` aggregate with the
/// supplied claim.
///
/// # Errors
/// Rejects absent/divergent evidence, report or claim drift, stale authority, or an operation
/// identity that belongs to another publication.
#[allow(clippy::too_many_arguments, reason = "publication owners and exact fences stay explicit")]
pub fn reconcile_interrupted_publication(
    journal: &mut SqliteJournal,
    evidence_store: &EvidenceStore,
    artifact_store: &ArtifactStore,
    state: &DebuggerState,
    report: &ValidatedReport,
    artifact: FinalizedReportArtifact,
    report_commit_position: u64,
    claim: PublicationDirectiveClaim,
    ids: TransitionIds,
) -> Result<PublicationExecution, DebuggerError> {
    let (durable_report, draft, publication) =
        publication_plan(state, report, report_commit_position)?;
    let directive = claim.directive();
    if artifact.report_id() != report.id()
        || artifact.payload_digest() != report.digest()
        || artifact.artifact_digest().sha256() != durable_report.digest()
        || artifact.size() != durable_report.size()
        || directive.job_id() != state.job_id()
        || directive.report() != durable_report
    {
        return Err(binding(
            "interrupted publication report, claim, or artifact differs",
        ));
    }
    verify_report_artifact(artifact_store, report, artifact)?;
    let evidence = evidence_store
        .load(publication.evidence_id())
        .map_err(evidence_error)?
        .ok_or_else(|| recovery("interrupted publication evidence is absent"))?;
    validate_recovered_evidence(&evidence, &draft)?;
    if let Some(operation) = recover_claimed_publication(
        journal,
        state.job_id(),
        durable_report,
        publication,
        claim,
        ids,
    )? {
        return Ok(PublicationExecution {
            evidence,
            committed: CommittedDebuggerTransition::new(operation),
        });
    }
    if state.phase() != DebuggerPhase::ReportReady {
        return Err(binding(
            "unsettled publication reconciliation requires report-ready state",
        ));
    }
    let command = publication_command(state, publication, ids)?;
    let transition = decide(Some(state), &command)?;
    let operation = commit_debugger_settlement(journal, &command, &transition, claim)?;
    Ok(PublicationExecution {
        evidence,
        committed: CommittedDebuggerTransition::new(operation),
    })
}

/// Restores exact artifact/evidence owners for an already accepted terminal publication.
///
/// The original report and publication operation identities are recovered from immutable journal
/// history. Repair reuses those outcomes and does not append another aggregate event, reclaim an
/// acknowledged directive, or describe the historical report-ready checkpoint as current.
///
/// # Errors
/// Rejects nonterminal or divergent state, operation identity drift, malformed retained receipts,
/// noncanonical report bytes, corrupt evidence, or dependency repair failure.
pub fn repair_completed_publication(
    journal: &SqliteJournal,
    evidence_store: &mut EvidenceStore,
    artifact_store: &ArtifactStore,
    state: &DebuggerState,
    report: &ValidatedReport,
) -> Result<CompletedPublicationRepair, DebuggerError> {
    if state.phase() != DebuggerPhase::Published {
        return Err(binding("completed publication repair requires published state"));
    }
    let retained_publication = state
        .publication()
        .ok_or_else(|| recovery("published state has no publication record"))?;
    let (durable_report, draft, publication) =
        publication_plan(state, report, retained_publication.journal_position())?;
    if publication != retained_publication {
        return Err(recovery(
            "published state differs from its exact report and evidence identities",
        ));
    }
    let (report_ids, publication_ids) =
        completed_publication_ids(journal, state, publication)?;
    let publication_operation = load_completed_publication_operation(
        journal,
        state,
        durable_report,
        publication,
        publication_ids,
    )?;
    let report_operation = load_report_operation(
        journal,
        state,
        durable_report,
        publication.journal_position(),
        report_ids,
    )?;
    if publication_operation.event().previous_event() != Some(report_operation.event().id()) {
        return Err(recovery(
            "accepted publication is detached from the retained report operation",
        ));
    }
    let existing_evidence = match evidence_store.load(publication.evidence_id()) {
        Ok(value) => value,
        Err(error) if error.recovery() == EvidenceRecoveryAction::Retry => {
            return Err(evidence_error(error));
        }
        Err(error) => return Err(evidence_integrity(error)),
    };
    if let Some(existing) = existing_evidence.as_ref() {
        validate_recovered_evidence(existing, &draft)?;
    }
    let artifact = finalize_report_artifact(artifact_store, report, report_ids.event_id())?;
    if artifact.artifact_digest().sha256() != publication.artifact_digest()
        || artifact.size() != publication.artifact_size()
        || report_operation.event().id() != report_ids.event_id()
    {
        return Err(recovery(
            "restored report artifact differs from the accepted publication",
        ));
    }
    let evidence = if let Some(existing) = existing_evidence {
        existing
    } else {
        let export = journal.integrity_export().map_err(journal_error)?;
        evidence_store
            .admit(draft.clone(), &export, artifact_store)
            .map_err(evidence_repair_error)?
    };
    validate_recovered_evidence(&evidence, &draft)?;
    if evidence.id() != publication.evidence_id() {
        return Err(recovery(
            "restored evidence identity differs from the accepted publication",
        ));
    }
    Ok(CompletedPublicationRepair {
        artifact,
        evidence,
        publication: CommittedDebuggerTransition::new(publication_operation),
    })
}

fn completed_publication_ids(
    journal: &SqliteJournal,
    state: &DebuggerState,
    publication: PublicationRecord,
) -> Result<(TransitionIds, TransitionIds), DebuggerError> {
    let records = journal
        .records_for_aggregate(debugger_aggregate_key(state.job_id())?)
        .map_err(journal_error)?;
    let report = records
        .iter()
        .find(|record| record.global_position() == publication.journal_position())
        .ok_or_else(|| recovery("published report journal position is absent"))?;
    let settled = records
        .last()
        .ok_or_else(|| recovery("published debugger aggregate has no immutable events"))?;
    if settled.event_id() != state.last_event_id()
        || settled.sequence().get() != state.sequence()
        || report.aggregate() != settled.aggregate()
        || report.sequence().get().checked_add(1) != Some(settled.sequence().get())
    {
        return Err(recovery(
            "published report and settlement positions are not the terminal event pair",
        ));
    }
    Ok((
        TransitionIds::new(report.command_id(), report.event_id()),
        TransitionIds::new(settled.command_id(), settled.event_id()),
    ))
}

fn publication_plan(
    state: &DebuggerState,
    report: &ValidatedReport,
    report_commit_position: u64,
) -> Result<(crate::ReportRecord, EvidenceDraft, PublicationRecord), DebuggerError> {
    if !matches!(state.phase(), DebuggerPhase::ReportReady | DebuggerPhase::Published) {
        return Err(binding(
            "publication dependency operation requires report-ready or published state",
        ));
    }
    let durable_report = state
        .report()
        .ok_or_else(|| binding("publication state has no durable report"))?;
    let expected_report = report_record(report)?;
    if durable_report != expected_report || report_commit_position == 0 {
        return Err(binding(
            "validated report or journal position differs from durable publication state",
        ));
    }
    let evidence_id = report_evidence_id(report)?;
    let draft = EvidenceDraft::new(
        evidence_id,
        EvidenceKind::new("debugger-report").map_err(evidence_error)?,
        EvidenceSource::new("peritus-debugger").map_err(evidence_error)?,
        *state.revision(),
        report_commit_position,
        report.digest(),
        vec![ArtifactDigest::from_sha256(durable_report.digest())],
        report.report().supersedes().into_iter().collect(),
    )
    .map_err(evidence_error)?;
    let publication = PublicationRecord::new(
        report.id(),
        durable_report.digest(),
        durable_report.size(),
        evidence_id,
        report_commit_position,
    )?;
    Ok((durable_report, draft, publication))
}

fn observe_artifact(
    artifact_store: &ArtifactStore,
    durable_report: crate::ReportRecord,
    report: &ValidatedReport,
) -> Result<PublicationDependencyStatus, DebuggerError> {
    if report_record(report)? != durable_report {
        return Err(recovery(
            "validated artifact root differs from the durable report",
        ));
    }
    let mut missing = false;
    let mut unavailable = false;
    for object in report.artifact_objects() {
        let digest = ArtifactDigest::from_sha256(object.digest());
        match artifact_store.verify(digest) {
            Ok(metadata) if metadata.size() == object.size() => {}
            Ok(_) => {
                return Err(recovery(
                    "verified report object size differs from the durable index",
                ));
            }
            Err(error) if error.code() == ErrorCode::MissingArtifact => missing = true,
            Err(error) if error.recovery_class() == RecoveryClass::Retry => unavailable = true,
            Err(error) => return Err(artifact_integrity(error)),
        }
    }
    if unavailable {
        Ok(PublicationDependencyStatus::Unavailable)
    } else if missing {
        Ok(PublicationDependencyStatus::Missing)
    } else {
        Ok(PublicationDependencyStatus::Verified)
    }
}

fn recover_claimed_publication(
    journal: &SqliteJournal,
    job_id: crate::DebuggerJobId,
    report: crate::ReportRecord,
    publication: PublicationRecord,
    claim: PublicationDirectiveClaim,
    ids: TransitionIds,
) -> Result<Option<crate::CommittedDebuggerOperation>, DebuggerError> {
    let Some(operation) = load_debugger_operation(journal, job_id, ids.command_id())? else {
        return Ok(None);
    };
    validate_recovered_claim(
        operation.receipt(),
        DebuggerCommitMode::Settlement,
        claim.directive().outbox_id()?,
        claim.fence(),
        DebuggerOperation::PublishEvidence,
    )?;
    if operation.event().id() != ids.event_id()
        || operation.event().command_id() != ids.command_id()
        || !matches!(
            operation.event().kind(),
            DebuggerEventKind::PublicationRecorded { publication: observed }
                if *observed == publication
        )
        || operation.historical_state().report() != Some(report)
        || operation.historical_state().publication() != Some(publication)
    {
        return Err(recovery(
            "recovered publication differs from the exact retry request",
        ));
    }
    Ok(Some(operation))
}

fn load_completed_publication_operation(
    journal: &SqliteJournal,
    state: &DebuggerState,
    report: crate::ReportRecord,
    publication: PublicationRecord,
    ids: TransitionIds,
) -> Result<crate::CommittedDebuggerOperation, DebuggerError> {
    let operation = load_debugger_operation(journal, state.job_id(), ids.command_id())?
        .ok_or_else(|| recovery("published state has no retained publication operation"))?;
    if !matches!(
        operation.receipt().mode(),
        DebuggerCommitMode::Settlement | DebuggerCommitMode::Legacy
    )
        || operation.event().id() != ids.event_id()
        || operation.event().command_id() != ids.command_id()
        || !matches!(
            operation.event().kind(),
            DebuggerEventKind::PublicationRecorded { publication: observed }
                if *observed == publication
        )
        || operation.historical_state() != state
        || operation.current_state() != state
        || operation.historical_state().report() != Some(report)
    {
        return Err(recovery(
            "retained publication operation differs from current published state",
        ));
    }
    Ok(operation)
}

fn load_report_operation(
    journal: &SqliteJournal,
    state: &DebuggerState,
    report: crate::ReportRecord,
    report_commit_position: u64,
    ids: TransitionIds,
) -> Result<crate::CommittedDebuggerOperation, DebuggerError> {
    let operation = load_debugger_operation(journal, state.job_id(), ids.command_id())?
        .ok_or_else(|| recovery("published state has no retained report operation"))?;
    if !matches!(
        operation.receipt().mode(),
        DebuggerCommitMode::Ordinary | DebuggerCommitMode::Legacy
    )
        || operation.event().id() != ids.event_id()
        || operation.event().command_id() != ids.command_id()
        || operation.batch().last_position() != report_commit_position
        || !matches!(
            operation.event().kind(),
            DebuggerEventKind::ReportCompleted { report: observed } if *observed == report
        )
        || operation.historical_state().phase() != DebuggerPhase::ReportReady
        || operation.historical_state().report() != Some(report)
        || operation.current_state() != state
    {
        return Err(recovery(
            "retained report operation differs from current published state",
        ));
    }
    Ok(operation)
}

fn publication_command(
    state: &DebuggerState,
    publication: PublicationRecord,
    ids: TransitionIds,
) -> Result<DebuggerCommand, DebuggerError> {
    DebuggerCommand::new(
        ids.command_id(),
        ids.event_id(),
        state.job_id(),
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.query_digest(),
        DebuggerCommandKind::RecordPublication { publication },
    )
}

fn validate_recovered_evidence(
    evidence: &EvidenceRecord,
    draft: &EvidenceDraft,
) -> Result<(), DebuggerError> {
    if evidence.id() != draft.id()
        || evidence.kind() != draft.kind()
        || evidence.source() != draft.source()
        || evidence.revision() != draft.revision()
        || evidence.provenance().global_position() != draft.journal_position()
        || evidence.payload_digest() != draft.payload_digest()
        || evidence.artifacts() != draft.artifacts()
        || evidence.causes() != draft.causes()
    {
        return Err(recovery(
            "recovered publication evidence differs from the original admission",
        ));
    }
    Ok(())
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
fn artifact_integrity(error: impl core::fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Artifact,
        DebuggerOperation::PublishArtifact,
        DebuggerRecovery::Quarantine,
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
fn evidence_integrity(error: impl core::fmt::Display) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Evidence,
        DebuggerOperation::PublishEvidence,
        DebuggerRecovery::Quarantine,
        error.to_string(),
    )
}
fn evidence_repair_error(error: EvidenceError) -> DebuggerError {
    let recovery = match error.recovery() {
        EvidenceRecoveryAction::Retry => DebuggerRecovery::Retry,
        EvidenceRecoveryAction::RepairDependency => DebuggerRecovery::RepairDependency,
        EvidenceRecoveryAction::CorrectInput
        | EvidenceRecoveryAction::RebuildCatalog
        | EvidenceRecoveryAction::ObtainFreshEvidence => DebuggerRecovery::Quarantine,
    };
    DebuggerError::new(
        DebuggerErrorKind::Evidence,
        DebuggerOperation::PublishEvidence,
        recovery,
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

fn recovery(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::Recovery,
        DebuggerOperation::Recover,
        DebuggerRecovery::Quarantine,
        detail,
    )
}
