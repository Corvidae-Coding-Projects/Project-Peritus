//! Startup reconciliation for durable terminal candidate handoffs.

use std::{collections::BTreeMap, path::PathBuf};

use peritus_app_protocol::{ProductDeliverable, ProductRunPhase};
use peritus_product_runner::ProductRunner;
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceStatus, QualificationEvidence,
    RunDisposition, SettlementReducer,
};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

use super::{
    ProductRunServiceError, RunRecord, persistence::persist_record, snapshot::replace_snapshot,
};
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

pub(super) fn reconcile_restored_candidates(
    directory: &std::path::Path,
    records: &mut BTreeMap<RunId, RunRecord>,
    workspaces: &BTreeMap<WorkspaceId, PathBuf>,
) -> Result<(), DaemonError> {
    for record in records.values_mut().filter(|record| terminal_candidate(record)) {
        let Some(root) = workspaces.get(&record.request.workspace_id()) else {
            mark_unavailable(record, "configured workspace is unavailable after restart")
                .map_err(persistence_error)?;
            persist_record(directory, record).map_err(persistence_error)?;
            continue;
        };
        let current = if let Ok(current) = ProductRunner::candidate_digest(root) {
            current
        } else {
            mark_unavailable(record, "candidate workspace could not be validated after restart")
                .map_err(persistence_error)?;
            persist_record(directory, record).map_err(persistence_error)?;
            continue;
        };
        let expected = record.checkpoint.as_ref().expect("terminal candidate has checkpoint");
        if current != expected.identity().candidate_digest() {
            mark_stale(record, current).map_err(persistence_error)?;
            persist_record(directory, record).map_err(persistence_error)?;
        }
    }
    Ok(())
}

fn terminal_candidate(record: &RunRecord) -> bool {
    record.snapshot.phase().terminal()
        && record.checkpoint.is_some()
        && record.snapshot.deliverable().is_some_and(|deliverable| {
            deliverable.commit_revision().is_empty() && !deliverable.discarded()
        })
}

fn mark_stale(
    record: &mut RunRecord,
    current_digest: Sha256Digest,
) -> Result<(), ProductRunServiceError> {
    let previous = record.checkpoint.ok_or(ProductRunServiceError::InvalidState)?;
    let sequence = previous
        .identity()
        .checkpoint_sequence()
        .checked_add(1)
        .ok_or(ProductRunServiceError::InvalidState)?;
    let identity = CandidateIdentity::new(
        previous.identity().run_id(),
        previous.identity().workspace_id(),
        current_digest,
        previous.identity().conversation_revision(),
        sequence,
    )
    .map_err(|_| ProductRunServiceError::InvalidState)?;
    let checkpoint = CandidateCheckpoint::new(
        identity,
        CandidateStage::Changed,
        stale(previous.gates()),
        stale(previous.obligations()),
        stale(previous.review()),
    )
    .map_err(|_| ProductRunServiceError::InvalidState)?;
    let cause = record.settlement.ok_or(ProductRunServiceError::InvalidState)?.cause();
    let mut reducer = SettlementReducer::new();
    reducer.observe(checkpoint).map_err(|_| ProductRunServiceError::InvalidState)?;
    let settlement = reducer.settle(cause).map_err(|_| ProductRunServiceError::InvalidState)?;
    let deliverable = reset_for_current_candidate(
        record.snapshot.deliverable().ok_or(ProductRunServiceError::InvalidState)?,
        CandidateStage::Changed,
    )?;
    let phase = match settlement.disposition() {
        RunDisposition::Accepted => ProductRunPhase::Complete,
        RunDisposition::CandidateAvailable | RunDisposition::FailedNoCandidate => {
            ProductRunPhase::Failed
        }
        RunDisposition::WaitingForUser => ProductRunPhase::WaitingForUser,
        RunDisposition::Cancelled => ProductRunPhase::Cancelled,
        RunDisposition::RecoveryRequired => ProductRunPhase::RecoveryRequired,
    };
    record.snapshot = replace_snapshot(
        &record.snapshot,
        phase,
        "Candidate changed after restart; qualification evidence is stale",
        "The managed workspace no longer matches the candidate that was checked. Inspect it and continue to requalify the current files.",
    )?
    .with_deliverable(deliverable);
    record.checkpoint = Some(checkpoint);
    record.settlement = Some(settlement);
    record.resume = None;
    record.candidate_actionable = false;
    "workspace changed after candidate settlement".clone_into(&mut record.interruption_cause);
    record.remaining_work = vec![
        "inspect the changed workspace".to_owned(),
        "continue the run to reacquire checks and review".to_owned(),
    ];
    Ok(())
}

fn mark_unavailable(record: &mut RunRecord, cause: &str) -> Result<(), ProductRunServiceError> {
    record.snapshot = replace_snapshot(
        &record.snapshot,
        record.snapshot.phase(),
        "Candidate workspace validation is unavailable",
        cause,
    )?;
    cause.clone_into(&mut record.interruption_cause);
    record.candidate_actionable = false;
    if !record.remaining_work.iter().any(|item| item.contains("workspace")) {
        record.remaining_work.push("restore access to the managed workspace".to_owned());
    }
    Ok(())
}

const fn stale(
    evidence: &EvidenceStatus<QualificationEvidence>,
) -> EvidenceStatus<QualificationEvidence> {
    match evidence {
        EvidenceStatus::Missing => EvidenceStatus::Missing,
        EvidenceStatus::Current(record)
        | EvidenceStatus::Failed(record)
        | EvidenceStatus::Stale(record) => EvidenceStatus::Stale(*record),
    }
}

fn reset_for_current_candidate(
    value: &ProductDeliverable,
    qualification: CandidateStage,
) -> Result<ProductDeliverable, ProductRunServiceError> {
    ProductDeliverable::candidate(
        value.workspace_path().to_owned(),
        value.changed_paths().to_vec(),
        value.successful_commands().to_vec(),
        value.run_instructions().to_owned(),
        qualification,
    )
    .map_err(|_| ProductRunServiceError::InvalidMessage)
}

fn persistence_error(_error: ProductRunServiceError) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "reconcile restored product candidate",
        "durable candidate state could not be updated after workspace validation",
    )
}
