//! Provenance-checked evidence admission and publication settlement.

use peritus_artifact_store::{
    ArtifactStore, ErrorCode as ArtifactErrorCode, RecoveryClass as ArtifactRecoveryClass,
};
use peritus_evidence::{
    EvidenceDraft, EvidenceKind, EvidenceRecord, EvidenceSource, EvidenceStore,
    RecoveryAction as EvidenceRecoveryAction,
};
use peritus_journal::{
    OutboxDeliveryStatus, OutboxId, RecoveryClass as JournalRecoveryClass, SqliteJournal,
};
use peritus_types::{EvidenceId, Sha256Digest};

use crate::{
    CommittedEvaluationOperation, EvaluationCommand, EvaluationCommandKind,
    EvaluationCommitMode, EvaluationError, EvaluationErrorKind, EvaluationEventKind,
    EvaluationOperation, EvaluationOperationReceipt, EvaluationPhase, EvaluationRecovery,
    EvaluationState, PublicationCancellationRecord, PublicationDirective,
    PublicationDirectiveClaim, PublicationDirectiveDelivery, PublicationRecord, ReportRecord,
    ValidatedEvaluationReport, commit_evaluation_settlement, decide, evaluation_aggregate_key,
    load_evaluation_operation,
};

use super::{
    CommittedEvaluationTransition, FinalizedEvaluationArtifact, TransitionIds,
    PublicationDependencyStatus, PublicationDirectiveObservation,
    PublicationRecoveryObservation, recover_claimed_operation,
};

/// Admitted evidence plus exact atomic C0 publication settlement.
#[derive(Debug)]
pub struct PublicationExecution {
    evidence: EvidenceRecord,
    committed: CommittedEvaluationTransition,
}

/// Exact immutable operations and acknowledged directive that own a terminal publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PublicationOwnershipReceipt {
    campaign_id: crate::EvaluationCampaignId,
    report: ReportRecord,
    publication: PublicationRecord,
    report_operation: EvaluationOperationReceipt,
    publication_operation: EvaluationOperationReceipt,
    outbox_id: OutboxId,
    fence: u64,
}

impl PublicationOwnershipReceipt {
    /// Original report-completion operation receipt.
    #[must_use]
    pub const fn report_operation(self) -> EvaluationOperationReceipt {
        self.report_operation
    }
    /// Exact publication-settlement operation receipt.
    #[must_use]
    pub const fn publication_operation(self) -> EvaluationOperationReceipt {
        self.publication_operation
    }
    /// Stable publication directive identity acknowledged by the settlement.
    #[must_use]
    pub const fn outbox_id(self) -> OutboxId {
        self.outbox_id
    }
    /// Positive delivery fence acknowledged by the settlement.
    #[must_use]
    pub const fn fence(self) -> u64 {
        self.fence
    }
    pub(crate) fn matches_state(self, state: &EvaluationState) -> bool {
        self.campaign_id == state.campaign_id()
            && state.report() == Some(self.report)
            && state.publication() == Some(self.publication)
            && state.phase() == EvaluationPhase::Published
    }
}

/// Restored or already-retained publication intent under its original report operation.
#[derive(Debug)]
pub struct PublicationIntentRecovery {
    report_operation: CommittedEvaluationOperation,
    delivery: PublicationDirectiveDelivery,
}

impl PublicationIntentRecovery {
    /// Original immutable report-completion operation.
    #[must_use]
    pub const fn report_operation(&self) -> &CommittedEvaluationOperation {
        &self.report_operation
    }
    /// Same canonical directive with its preserved or newly pending delivery state.
    #[must_use]
    pub const fn delivery(&self) -> PublicationDirectiveDelivery {
        self.delivery
    }
    /// Consumes the recovery result.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (CommittedEvaluationOperation, PublicationDirectiveDelivery) {
        (self.report_operation, self.delivery)
    }
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
    let (durable_report, draft, publication) =
        publication_plan(state, report, report_commit_position)?;
    let _report_operation = load_report_operation(
        journal,
        state,
        durable_report,
        report_commit_position,
    )?;
    let directive = *claim.directive();
    if durable_report.id() != report.id()
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
    let kind = EvaluationCommandKind::RecordPublication { publication };
    if let Some(operation) = recover_claimed_operation(
        journal,
        state.campaign_id(),
        ids,
        &kind,
        EvaluationCommitMode::Settlement,
        claim.outbox_id()?,
        claim.fence(),
    )? {
        if operation.historical_state().report() != Some(durable_report)
            || operation.historical_state().publication() != Some(publication)
            || operation.historical_state().phase() != EvaluationPhase::Published
        {
            return Err(recovery(
                "recovered publication differs from its historical outcome",
            ));
        }
        let evidence = evidence_store
            .load(publication.evidence_id())
            .map_err(evidence_error)?
            .ok_or_else(|| recovery("recovered publication evidence is absent"))?;
        validate_recovered_evidence(&evidence, &draft)?;
        return Ok(PublicationExecution {
            evidence,
            committed: CommittedEvaluationTransition::new(operation),
        });
    }
    if state.phase() != EvaluationPhase::ReportReady {
        return Err(binding("publication state is not report-ready"));
    }
    let export = journal
        .integrity_export_for_head(report_commit_position)
        .map_err(journal_error)?;
    let evidence = evidence_store.admit(draft, &export, artifact_store).map_err(evidence_error)?;
    if evidence.id() != publication.evidence_id() {
        return Err(recovery("admitted evidence identity differs from publication"));
    }
    let command = EvaluationCommand::new(
        ids.command_id(),
        ids.event_id(),
        state.campaign_id(),
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.profile_digest(),
        kind,
    )?;
    let transition = decide(Some(state), &command)?;
    let operation = commit_evaluation_settlement(journal, &command, &transition, claim)?;
    Ok(PublicationExecution {
        evidence,
        committed: CommittedEvaluationTransition::new(operation),
    })
}

/// Settles an interrupted publication from already admitted exact evidence.
///
/// This path never calls evidence admission. It recovers an accepted settlement through its
/// immutable receipt or atomically records publication against the still-live original claim.
///
/// # Errors
/// Rejects absent/divergent evidence, report operation drift, stale claim ownership, or C0 failure.
#[allow(clippy::too_many_arguments, reason = "publication owners and exact fences stay explicit")]
pub fn reconcile_interrupted_publication(
    journal: &mut SqliteJournal,
    evidence_store: &EvidenceStore,
    artifact_store: &ArtifactStore,
    state: &EvaluationState,
    report: &ValidatedEvaluationReport,
    artifact: FinalizedEvaluationArtifact,
    report_commit_position: u64,
    claim: PublicationDirectiveClaim,
    ids: TransitionIds,
) -> Result<PublicationExecution, EvaluationError> {
    let (durable_report, draft, publication) =
        publication_plan(state, report, report_commit_position)?;
    let _report_operation = load_report_operation(
        journal,
        state,
        durable_report,
        report_commit_position,
    )?;
    let directive = *claim.directive();
    if durable_report.artifact() != artifact.artifact_digest()
        || durable_report.size() != artifact.size()
        || artifact.report_id() != report.id()
        || artifact.payload_digest() != report.digest()
        || directive.campaign_id() != state.campaign_id()
        || directive.report() != durable_report
    {
        return Err(binding(
            "interrupted publication report, claim, or artifact differs",
        ));
    }
    artifact_store.verify(artifact.artifact_digest()).map_err(artifact_error)?;
    let evidence = evidence_store
        .load(publication.evidence_id())
        .map_err(evidence_error)?
        .ok_or_else(|| recovery("interrupted publication evidence is absent"))?;
    validate_recovered_evidence(&evidence, &draft)?;
    let kind = EvaluationCommandKind::RecordPublication { publication };
    if let Some(operation) = recover_claimed_operation(
        journal,
        state.campaign_id(),
        ids,
        &kind,
        EvaluationCommitMode::Settlement,
        claim.outbox_id()?,
        claim.fence(),
    )? {
        if operation.historical_state().report() != Some(durable_report)
            || operation.historical_state().publication() != Some(publication)
            || operation.historical_state().phase() != EvaluationPhase::Published
        {
            return Err(recovery(
                "recovered interrupted publication differs from its historical outcome",
            ));
        }
        return Ok(PublicationExecution {
            evidence,
            committed: CommittedEvaluationTransition::new(operation),
        });
    }
    if state.phase() != EvaluationPhase::ReportReady {
        return Err(binding(
            "unsettled publication reconciliation requires report-ready state",
        ));
    }
    let command = EvaluationCommand::new(
        ids.command_id(),
        ids.event_id(),
        state.campaign_id(),
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.profile_digest(),
        kind,
    )?;
    let transition = decide(Some(state), &command)?;
    let operation = commit_evaluation_settlement(journal, &command, &transition, claim)?;
    Ok(PublicationExecution {
        evidence,
        committed: CommittedEvaluationTransition::new(operation),
    })
}

/// Restores a missing publication row from the original accepted `CompleteReport` operation.
///
/// The report artifact, immutable operation receipt, event semantics, historical successor,
/// artifact dependency, and current report-ready state must all agree. Existing delivery state is
/// preserved exactly. A missing row is inserted pending at the original producing position.
///
/// # Errors
/// Rejects any owner drift, a non-report-ready state, an invalid observation tick, or C0 failure.
pub fn reconcile_publication_intent(
    journal: &mut SqliteJournal,
    artifact_store: &ArtifactStore,
    state: &EvaluationState,
    observed_at: u64,
) -> Result<PublicationIntentRecovery, EvaluationError> {
    if state.phase() != EvaluationPhase::ReportReady || state.publication().is_some() {
        return Err(binding(
            "publication-intent recovery requires unsettled report-ready state",
        ));
    }
    let report = state
        .report()
        .ok_or_else(|| recovery("report-ready state has no retained report"))?;
    let report_position = current_report_position(journal, state)?;
    let operation = load_report_operation(journal, state, report, report_position)?;
    if operation.current_state() != state {
        return Err(recovery(
            "original report operation no longer produces the supplied current state",
        ));
    }
    let metadata = artifact_store.verify(report.artifact()).map_err(artifact_error)?;
    if metadata.size() != report.size() {
        return Err(recovery(
            "report artifact owner differs from the immutable report operation",
        ));
    }
    let directive = PublicationDirective::new(state.campaign_id(), report);
    let draft = directive.outbox_draft()?;
    let message = journal
        .reconcile_outbox(operation.batch(), &draft)
        .map_err(journal_error)?;
    if message.producing_position() != report_position {
        return Err(recovery(
            "reconciled publication directive has another producing operation",
        ));
    }
    let delivery = PublicationDirectiveDelivery::from_message(&message, observed_at)?;
    Ok(PublicationIntentRecovery { report_operation: operation, delivery })
}

/// Observes exact publication dependencies, directive ownership, and terminal settlement receipt.
///
/// # Errors
/// Rejects divergent identities, corrupt owners, invalid report provenance, or malformed history.
pub fn observe_publication_recovery(
    journal: &SqliteJournal,
    evidence_store: &EvidenceStore,
    artifact_store: &ArtifactStore,
    state: &EvaluationState,
    report: &ValidatedEvaluationReport,
    observed_at: u64,
) -> Result<PublicationRecoveryObservation, EvaluationError> {
    if !matches!(state.phase(), EvaluationPhase::ReportReady | EvaluationPhase::Published) {
        return Err(binding(
            "publication observation requires report-ready or published state",
        ));
    }
    let durable_report = state
        .report()
        .ok_or_else(|| recovery("publication state has no retained report"))?;
    let report_position = state
        .publication()
        .map_or_else(|| current_report_position(journal, state), |value| {
            Ok(value.report_commit_position())
        })?;
    let _report_operation =
        load_report_operation(journal, state, durable_report, report_position)?;
    let (planned_report, draft, publication) =
        publication_plan(state, report, report_position)?;
    if planned_report != durable_report {
        return Err(recovery(
            "publication observation report differs from immutable state",
        ));
    }
    let artifact = match artifact_store.verify(durable_report.artifact()) {
        Ok(metadata) if metadata.size() == durable_report.size() => {
            PublicationDependencyStatus::Verified
        }
        Ok(_) => return Err(recovery("report artifact metadata differs from durable report")),
        Err(error) if error.code() == ArtifactErrorCode::MissingArtifact => {
            PublicationDependencyStatus::Missing
        }
        Err(error) if error.recovery_class() == ArtifactRecoveryClass::Retry => {
            PublicationDependencyStatus::Unavailable
        }
        Err(error) => return Err(artifact_error(error)),
    };
    let evidence = match evidence_store.load(publication.evidence_id()) {
        Ok(Some(record)) => {
            validate_recovered_evidence(&record, &draft)?;
            PublicationDependencyStatus::Verified
        }
        Ok(None) => PublicationDependencyStatus::Missing,
        Err(error) if error.recovery() == EvidenceRecoveryAction::Retry => {
            PublicationDependencyStatus::Unavailable
        }
        Err(error) => return Err(evidence_error(error)),
    };
    let directive_value = PublicationDirective::new(state.campaign_id(), durable_report);
    let directive = match journal.outbox_message(directive_value.outbox_id()?) {
        Ok(Some(message)) => {
            if message.producing_position() != report_position {
                return Err(recovery(
                    "publication directive is owned by another producing operation",
                ));
            }
            PublicationDirectiveObservation::Observed(
                PublicationDirectiveDelivery::from_message(&message, observed_at)?,
            )
        }
        Ok(None) => PublicationDirectiveObservation::Missing,
        Err(error) if error.recovery() == JournalRecoveryClass::Retry => {
            PublicationDirectiveObservation::Unavailable
        }
        Err(error) => return Err(journal_error(error)),
    };
    let ownership = if state.phase() == EvaluationPhase::Published {
        Some(load_publication_ownership(journal, state)?)
    } else {
        None
    };
    Ok(PublicationRecoveryObservation::new(
        state.campaign_id(),
        durable_report,
        publication,
        publication.evidence_id(),
        artifact,
        evidence,
        directive,
        ownership,
    ))
}

/// Loads and verifies the exact report operation, acknowledged directive, and publication
/// settlement that own a terminal published state.
///
/// # Errors
/// Rejects missing or nonadjacent operations, receipt/claim drift, an unacknowledged directive,
/// evidence identity drift, or a historical outcome that differs from current terminal state.
pub fn load_publication_ownership(
    journal: &SqliteJournal,
    state: &EvaluationState,
) -> Result<PublicationOwnershipReceipt, EvaluationError> {
    if state.phase() != EvaluationPhase::Published {
        return Err(binding("publication ownership requires published state"));
    }
    let report = state
        .report()
        .ok_or_else(|| recovery("published state has no retained report"))?;
    let publication = state
        .publication()
        .ok_or_else(|| recovery("published state has no publication record"))?;
    if publication.report_id() != report.id() {
        return Err(recovery("published report and publication identities differ"));
    }
    let records = journal
        .records_for_aggregate(evaluation_aggregate_key(state.campaign_id())?)
        .map_err(journal_error)?;
    let report_record = records
        .iter()
        .find(|record| record.global_position() == publication.report_commit_position())
        .ok_or_else(|| recovery("published report operation position is absent"))?;
    let settlement_record = records
        .last()
        .ok_or_else(|| recovery("published campaign has no immutable events"))?;
    if settlement_record.event_id() != state.last_event_id()
        || settlement_record.sequence().get() != state.sequence()
        || report_record.aggregate() != settlement_record.aggregate()
        || report_record.sequence().get().checked_add(1)
            != Some(settlement_record.sequence().get())
    {
        return Err(recovery(
            "report and publication settlement are not the terminal adjacent operations",
        ));
    }
    let report_operation = load_report_operation(
        journal,
        state,
        report,
        publication.report_commit_position(),
    )?;
    let settlement = load_evaluation_operation(
        journal,
        state.campaign_id(),
        settlement_record.command_id(),
    )?
    .ok_or_else(|| recovery("publication settlement operation is absent"))?;
    let EvaluationEventKind::Accepted(settlement_kind) = settlement.event().kind();
    if settlement.batch().last_position() != settlement_record.global_position()
        || settlement.event().id() != settlement_record.event_id()
        || settlement.event().previous_event() != Some(report_operation.event().id())
        || settlement_kind != &(EvaluationCommandKind::RecordPublication { publication })
        || settlement.historical_state() != state
        || settlement.current_state() != state
    {
        return Err(recovery(
            "publication settlement differs from immutable terminal state",
        ));
    }
    let directive = PublicationDirective::new(state.campaign_id(), report);
    let outbox_id = directive.outbox_id()?;
    let message = journal
        .outbox_message(outbox_id)
        .map_err(journal_error)?
        .ok_or_else(|| recovery("published directive owner row is absent"))?;
    if message.producing_position() != publication.report_commit_position() {
        return Err(recovery(
            "published directive belongs to another report operation",
        ));
    }
    let delivery = PublicationDirectiveDelivery::from_message(&message, 1)?;
    if delivery.directive() != directive
        || delivery.status() != OutboxDeliveryStatus::Acknowledged
    {
        return Err(recovery(
            "published directive is not the exact acknowledged report effect",
        ));
    }
    let fence = message
        .fence()
        .ok_or_else(|| recovery("acknowledged publication directive has no owner fence"))?;
    let settlement_receipt = settlement.receipt();
    match settlement_receipt.mode() {
        EvaluationCommitMode::Settlement => {
            let original = settlement_receipt
                .original_claim()
                .ok_or_else(|| recovery("publication settlement lost its original claim"))?;
            if original.outbox_id() != outbox_id || original.fence() != fence {
                return Err(recovery(
                    "publication settlement receipt differs from acknowledged ownership",
                ));
            }
        }
        EvaluationCommitMode::Legacy => {}
        EvaluationCommitMode::Ordinary | EvaluationCommitMode::Claimed => {
            return Err(recovery(
                "publication settlement has a nonsettlement operation receipt",
            ));
        }
    }
    Ok(PublicationOwnershipReceipt {
        campaign_id: state.campaign_id(),
        report,
        publication,
        report_operation: report_operation.receipt(),
        publication_operation: settlement_receipt,
        outbox_id,
        fence,
    })
}

/// Atomically acknowledges a claimed publication after cancellation won its state race.
///
/// Any evidence admitted before cancellation is retained as an identity fact without changing the
/// campaign to `Published` or granting downstream promotion authority.
///
/// # Errors
/// Rejects report/claim drift, the wrong cancellation origin, duplicate settlement, or C0 failure.
pub fn cancel_claimed_publication(
    journal: &mut SqliteJournal,
    state: &EvaluationState,
    claim: PublicationDirectiveClaim,
    observation_digest: Sha256Digest,
    admitted_evidence: Option<EvidenceId>,
    ids: TransitionIds,
) -> Result<CommittedEvaluationTransition, EvaluationError> {
    let report = state.report().ok_or_else(|| binding("cancelled publication has no report"))?;
    let directive = *claim.directive();
    if directive.campaign_id() != state.campaign_id() || directive.report() != report
    {
        return Err(binding("publication cancellation state, report, or claim differs"));
    }
    let cancellation =
        PublicationCancellationRecord::new(report, observation_digest, admitted_evidence);
    let kind = EvaluationCommandKind::SettlePublicationCancellation { cancellation };
    if let Some(operation) = recover_claimed_operation(
        journal,
        state.campaign_id(),
        ids,
        &kind,
        EvaluationCommitMode::Settlement,
        claim.outbox_id()?,
        claim.fence(),
    )? {
        if operation.historical_state().publication_cancellation() != Some(cancellation) {
            return Err(recovery(
                "recovered publication cancellation differs from its historical outcome",
            ));
        }
        return Ok(CommittedEvaluationTransition::new(operation));
    }
    if state.phase() != EvaluationPhase::Cancelling
        || state.cancellation_origin() != Some(EvaluationPhase::ReportReady)
        || state.publication_cancellation().is_some()
    {
        return Err(binding("publication cancellation state is not unsettled"));
    }
    let command = EvaluationCommand::new(
        ids.command_id(),
        ids.event_id(),
        state.campaign_id(),
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        state.profile_digest(),
        kind,
    )?;
    let transition = decide(Some(state), &command)?;
    let operation = commit_evaluation_settlement(journal, &command, &transition, claim)?;
    Ok(CommittedEvaluationTransition::new(operation))
}

fn publication_plan(
    state: &EvaluationState,
    report: &ValidatedEvaluationReport,
    report_commit_position: u64,
) -> Result<(ReportRecord, EvidenceDraft, PublicationRecord), EvaluationError> {
    if !matches!(state.phase(), EvaluationPhase::ReportReady | EvaluationPhase::Published) {
        return Err(binding(
            "publication dependency operation requires report-ready or published state",
        ));
    }
    let durable_report = state
        .report()
        .ok_or_else(|| binding("publication state has no durable report"))?;
    let report_size = u64::try_from(report.bytes().len())
        .map_err(|_| binding("validated report size is not representable"))?;
    if report.report().campaign_id() != state.campaign_id()
        || report.report().profile_digest() != state.profile_digest()
        || durable_report.id() != report.id()
        || durable_report.payload_digest() != report.digest()
        || durable_report.artifact().sha256() != peritus_codec::sha256(report.bytes())
        || durable_report.size() != report_size
        || report_commit_position == 0
    {
        return Err(binding(
            "validated report or report operation differs from durable publication state",
        ));
    }
    let evidence_id = report_evidence_id(report)?;
    let draft = EvidenceDraft::new(
        evidence_id,
        EvidenceKind::new("evaluation-report").map_err(evidence_error)?,
        EvidenceSource::new("peritus-eval").map_err(evidence_error)?,
        *state.revision(),
        report_commit_position,
        report.digest(),
        vec![durable_report.artifact()],
        Vec::new(),
    )
    .map_err(evidence_error)?;
    let publication =
        PublicationRecord::new(report.id(), evidence_id, report_commit_position)?;
    match state.phase() {
        EvaluationPhase::ReportReady if state.publication().is_none() => {}
        EvaluationPhase::Published if state.publication() == Some(publication) => {}
        EvaluationPhase::ReportReady | EvaluationPhase::Published => {
            return Err(binding(
                "publication record differs from the report and exact provenance",
            ));
        }
        _ => unreachable!("phase checked above"),
    }
    Ok((durable_report, draft, publication))
}

fn current_report_position(
    journal: &SqliteJournal,
    state: &EvaluationState,
) -> Result<u64, EvaluationError> {
    if state.phase() != EvaluationPhase::ReportReady {
        return Err(binding(
            "current report operation lookup requires report-ready state",
        ));
    }
    let records = journal
        .records_for_aggregate(evaluation_aggregate_key(state.campaign_id())?)
        .map_err(journal_error)?;
    let record = records
        .last()
        .ok_or_else(|| recovery("report-ready campaign has no immutable events"))?;
    if record.event_id() != state.last_event_id()
        || record.sequence().get() != state.sequence()
    {
        return Err(recovery(
            "report-ready checkpoint differs from its immutable head",
        ));
    }
    Ok(record.global_position())
}

fn load_report_operation(
    journal: &SqliteJournal,
    state: &EvaluationState,
    report: ReportRecord,
    report_commit_position: u64,
) -> Result<CommittedEvaluationOperation, EvaluationError> {
    let records = journal
        .records_for_aggregate(evaluation_aggregate_key(state.campaign_id())?)
        .map_err(journal_error)?;
    let record = records
        .iter()
        .find(|record| record.global_position() == report_commit_position)
        .ok_or_else(|| recovery("report operation journal position is absent"))?;
    let operation = load_evaluation_operation(journal, state.campaign_id(), record.command_id())?
        .ok_or_else(|| recovery("report operation receipt is absent"))?;
    let EvaluationEventKind::Accepted(kind) = operation.event().kind();
    if operation.batch().last_position() != report_commit_position
        || operation.event().id() != record.event_id()
        || kind != &(EvaluationCommandKind::CompleteReport { report })
        || operation.historical_state().phase() != EvaluationPhase::ReportReady
        || operation.historical_state().report() != Some(report)
        || operation.batch().artifact_dependencies()
            != [peritus_journal::ArtifactDependency::new(report.artifact().sha256())]
        || !matches!(
            operation.receipt().mode(),
            EvaluationCommitMode::Ordinary | EvaluationCommitMode::Legacy
        )
    {
        return Err(recovery(
            "report operation differs from immutable report ownership",
        ));
    }
    Ok(operation)
}

fn validate_recovered_evidence(
    evidence: &EvidenceRecord,
    draft: &EvidenceDraft,
) -> Result<(), EvaluationError> {
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
        "evaluation publication journal operation failed",
    )
}

const fn recovery(detail: &'static str) -> EvaluationError {
    EvaluationError::new(
        EvaluationErrorKind::Recovery,
        EvaluationOperation::Recover,
        EvaluationRecovery::Quarantine,
        detail,
    )
}
