//! Startup reconciliation for durable terminal candidate handoffs.

use std::{collections::BTreeMap, path::PathBuf};

use peritus_app_protocol::{ProductDeliverable, ProductRunPhase};
use peritus_product_runner::ProductRunner;
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, RunDisposition, SettlementReducer,
};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

use super::{
    ProductRunServiceError, RunRecord, snapshot::replace_snapshot,
};
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

pub(super) fn reconcile_restored_candidates(
    directory: &std::path::Path,
    records: &mut BTreeMap<RunId, RunRecord>,
    workspaces: &BTreeMap<WorkspaceId, PathBuf>,
) -> Result<(), DaemonError> {
    for record in records.values_mut().filter(|record| terminal_candidate(record)) {
        if record.resume_decode_pending() {
            // The opaque continuation root is still the sole authority for this exact checkpoint.
            // Advancing candidate lineage would strand that retained root on its predecessor.
            continue;
        }
        match super::deliverable::discard::recover_completed(directory, record) {
            Ok(true) => {
                persist_recovery(directory, record)?;
                continue;
            }
            Ok(false) => {}
            Err(error) => {
                mark_unavailable(record, &error.to_string()).map_err(persistence_error)?;
                persist_recovery(directory, record)?;
                continue;
            }
        }
        match super::deliverable::discard::Pending::read(directory, record) {
            Ok(Some(pending)) => {
                if let Err(error) = pending.inspect(directory, record) {
                    mark_unavailable(record, &error.to_string()).map_err(persistence_error)?;
                } else {
                    super::deliverable::discard::Pending::mark_interrupted(record)
                        .map_err(persistence_error)?;
                }
                persist_recovery(directory, record)?;
                continue;
            }
            Ok(None) => {}
            Err(error) => {
                mark_unavailable(record, &error.to_string()).map_err(persistence_error)?;
                persist_recovery(directory, record)?;
                continue;
            }
        }
        let Some(root) = workspaces.get(&record.request.workspace_id()) else {
            mark_unavailable(record, "configured workspace is unavailable after restart")
                .map_err(persistence_error)?;
            persist_recovery(directory, record)?;
            continue;
        };
        let current_repository = if let Ok(current) = ProductRunner::candidate_digest(root) {
            current
        } else {
            mark_unavailable(record, "candidate workspace could not be validated after restart")
                .map_err(persistence_error)?;
            persist_recovery(directory, record)?;
            continue;
        };
        let current_content = if let Ok(current) = ProductRunner::candidate_source_digest(root) {
            current
        } else {
            mark_unavailable(
                record,
                "candidate source content could not be validated after restart",
            )
            .map_err(persistence_error)?;
            persist_recovery(directory, record)?;
            continue;
        };
        let expected = record.checkpoint.as_ref().expect("terminal candidate has checkpoint");
        let execution_evidence_current = expected.identity().execution_digest().is_none()
            || record.resume.as_ref().is_some_and(|resume| {
                resume.checkpoint() == expected && resume.retains_current_gate_state()
            });
        if current_repository != expected.identity().repository_digest()
            || current_content != expected.identity().content_digest()
            || !execution_evidence_current
        {
            mark_stale(record, current_content, current_repository, None)
                .map_err(persistence_error)?;
            persist_recovery(directory, record)?;
        } else if record.resume.is_some() {
            project_reconciled(
                record,
                *expected,
                record
                    .settlement
                    .ok_or_else(|| persistence_error(ProductRunServiceError::InvalidState))?,
            )
            .map_err(persistence_error)?;
            persist_recovery(directory, record)?;
        }
    }
    Ok(())
}

fn persist_recovery(
    directory: &std::path::Path,
    record: &mut RunRecord,
) -> Result<(), DaemonError> {
    super::publication::persist_startup_record(
        directory,
        record,
        peritus_codec::sha256(b"reconcile-restored-product-candidate"),
    )
    .map_err(persistence_error)
}

fn terminal_candidate(record: &RunRecord) -> bool {
    record.snapshot.phase().terminal()
        && record.checkpoint.is_some()
        && record.snapshot.deliverable().is_some_and(|deliverable| {
            deliverable.commit_revision().is_empty() && !deliverable.discarded()
        })
}

pub(super) fn mark_stale(
    record: &mut RunRecord,
    current_content: Sha256Digest,
    current_repository: Sha256Digest,
    current_execution: Option<Sha256Digest>,
) -> Result<(), ProductRunServiceError> {
    let previous = record.checkpoint.ok_or(ProductRunServiceError::InvalidState)?;
    let checkpoint =
        changed_checkpoint(previous, current_content, current_repository, current_execution)?;
    let cause = record.settlement.ok_or(ProductRunServiceError::InvalidState)?.cause();
    let mut reducer = SettlementReducer::new();
    reducer.observe(checkpoint).map_err(|_| ProductRunServiceError::InvalidState)?;
    let settlement = reducer.settle(cause).map_err(|_| ProductRunServiceError::InvalidState)?;
    record.resume = record
        .resume
        .take()
        .map(|resume| resume.reconcile_candidate(checkpoint))
        .transpose()
        .map_err(|_| ProductRunServiceError::InvalidState)?;
    project_reconciled(record, checkpoint, settlement)
}

fn project_reconciled(
    record: &mut RunRecord,
    checkpoint: CandidateCheckpoint,
    settlement: peritus_run_settlement::RunSettlement,
) -> Result<(), ProductRunServiceError> {
    let deliverable = reset_for_current_candidate(
        record.snapshot.deliverable().ok_or(ProductRunServiceError::InvalidState)?,
        checkpoint.stage(),
    )?;
    let interrupted = record.snapshot.phase() == ProductRunPhase::RecoveryRequired;
    let phase = if interrupted {
        ProductRunPhase::RecoveryRequired
    } else {
        match settlement.disposition() {
            RunDisposition::Accepted => ProductRunPhase::Complete,
            RunDisposition::CandidateAvailable | RunDisposition::FailedNoCandidate => {
                ProductRunPhase::Failed
            }
            RunDisposition::WaitingForUser => ProductRunPhase::WaitingForUser,
            RunDisposition::Cancelled => ProductRunPhase::Cancelled,
            RunDisposition::RecoveryRequired => ProductRunPhase::RecoveryRequired,
        }
    };
    let status = if interrupted {
        record.snapshot.status()
    } else {
        "Candidate facts reconciled; affected qualification evidence is stale"
    };
    record.snapshot = replace_snapshot(
        &record.snapshot,
        phase,
        status,
        "The host reconciled source, repository, and execution facts. Inspect the retained evidence and continue to reacquire only the stale observations.",
    )?
    .with_deliverable(deliverable);
    record.checkpoint = Some(checkpoint);
    record.settlement = Some(settlement);
    record.candidate_actionable = checkpoint.is_qualified();
    "candidate facts reconciled after settlement".clone_into(&mut record.interruption_cause);
    record.remaining_work = if checkpoint.is_qualified() {
        Vec::new()
    } else {
        vec![
            "inspect the reconciled candidate facts".to_owned(),
            "continue the run to reacquire stale qualification evidence".to_owned(),
        ]
    };
    Ok(())
}

/// Advances an exact identity while retaining the original evidence as historical observations.
pub(super) fn changed_checkpoint(
    previous: CandidateCheckpoint,
    current_content: Sha256Digest,
    current_repository: Sha256Digest,
    current_execution: Option<Sha256Digest>,
) -> Result<CandidateCheckpoint, ProductRunServiceError> {
    let sequence = previous
        .identity()
        .checkpoint_sequence()
        .checked_add(1)
        .ok_or(ProductRunServiceError::InvalidState)?;
    let identity = CandidateIdentity::new(
        previous.identity().run_id(),
        previous.identity().workspace_id(),
        current_content,
        current_repository,
        current_execution,
        previous.identity().requirements_revision(),
        sequence,
    )
    .map_err(|_| ProductRunServiceError::InvalidState)?;
    previous.reobserve(identity).map_err(|_| ProductRunServiceError::InvalidState)
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

pub(super) fn reset_for_current_candidate(
    value: &ProductDeliverable,
    qualification: CandidateStage,
) -> Result<ProductDeliverable, ProductRunServiceError> {
    let current = ProductDeliverable::candidate(
        value.workspace_path().to_owned(),
        value.changed_paths().to_vec(),
        value.successful_commands().to_vec(),
        value.run_instructions().to_owned(),
        qualification,
    )
    .map_err(|_| ProductRunServiceError::InvalidMessage)?;
    // An exported patch is an immutable artifact of the original candidate.
    // Keep it reachable even when live files need fresh qualification.
    if value.export_path().is_empty() {
        Ok(current)
    } else {
        current
            .mark_exported(value.export_path().to_owned())
            .map_err(|_| ProductRunServiceError::InvalidMessage)
    }
}

fn persistence_error(_error: ProductRunServiceError) -> DaemonError {
    DaemonError::new(
        DaemonErrorCode::Storage,
        DaemonRecovery::Reconcile,
        "reconcile restored product candidate",
        "durable candidate state could not be updated after workspace validation",
    )
}
