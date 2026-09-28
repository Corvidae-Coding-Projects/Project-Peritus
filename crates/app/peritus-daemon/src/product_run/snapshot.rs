//! Product-run snapshot construction and state replacement.

use std::collections::BTreeMap;

use peritus_app_protocol::{
    AppResponsePayload, ProductRunPhase, ProductRunRequest, ProductRunSettlementSnapshot,
    ProductRunSnapshot,
};
use peritus_types::{RunId, WorkspaceId};

use super::{ProductRunServiceError, RunRecord};

impl super::ProductRunService {
    pub(crate) fn query_observations(
        &self,
        query: peritus_app_protocol::ProductRunQuery,
    ) -> Result<Vec<peritus_app_protocol::ProductRunObservation>, ProductRunServiceError> {
        let records = self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
        records
            .values()
            .rev()
            .filter(|record| query.run_id().is_none_or(|run| record.snapshot.run_id() == run))
            .take(peritus_app_protocol::MAX_PRODUCT_RUNS)
            .map(|record| {
                peritus_app_protocol::ProductRunObservation::new(
                    live_snapshot(record)?,
                    delivery_settlement(record),
                )
                .map_err(|_| ProductRunServiceError::InvalidState)
            })
            .collect()
    }
}

pub(super) fn project_snapshot(
    record: &RunRecord,
    snapshot: ProductRunSnapshot,
) -> Result<AppResponsePayload, ProductRunServiceError> {
    match delivery_settlement(record) {
        Some(settlement) => ProductRunSettlementSnapshot::new(snapshot, settlement)
            .map(AppResponsePayload::ProductRunSettled)
            .map_err(|_| ProductRunServiceError::InvalidState),
        None => Ok(AppResponsePayload::ProductRunAccepted(snapshot)),
    }
}

pub(super) fn project_collection(
    records: &BTreeMap<RunId, RunRecord>,
    snapshots: Vec<ProductRunSnapshot>,
) -> Result<AppResponsePayload, ProductRunServiceError> {
    let settled = snapshots
        .iter()
        .map(|snapshot| {
            records
                .get(&snapshot.run_id())
                .and_then(delivery_settlement)
                .map(|settlement| ProductRunSettlementSnapshot::new(snapshot.clone(), settlement))
                .transpose()
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ProductRunServiceError::InvalidState)?;
    if settled.iter().all(Option::is_some) {
        Ok(AppResponsePayload::ProductRunSettlements(settled.into_iter().flatten().collect()))
    } else {
        // Legacy collections cannot encode candidate qualification. Keep their status
        // visible without asserting acceptance; exact queries retain the full evidence.
        let summaries = snapshots.into_iter().map(legacy_summary).collect::<Result<_, _>>()?;
        Ok(AppResponsePayload::ProductRuns(summaries))
    }
}

fn legacy_summary(
    snapshot: ProductRunSnapshot,
) -> Result<ProductRunSnapshot, ProductRunServiceError> {
    if !snapshot.deliverable().is_some_and(|value| {
        value.qualification() != peritus_run_settlement::CandidateStage::Qualified
    }) {
        return Ok(snapshot);
    }
    ProductRunSnapshot::new(
        snapshot.run_id(),
        snapshot.workspace_id(),
        snapshot.providers(),
        snapshot.phase(),
        snapshot.cycle(),
        snapshot.task().to_owned(),
        snapshot.status().to_owned(),
        snapshot.diff().to_owned(),
        snapshot.gates().to_owned(),
        snapshot.review().to_owned(),
        snapshot.summary().to_owned(),
    )
    .map_err(|_| ProductRunServiceError::InvalidState)
}

/// The legacy settlement wire shape requires a managed deliverable for every checkpoint.
/// In-place execution retains that checkpoint internally but publishes its phase and evidence
/// through the ordinary snapshot, without inventing a Git handoff or changing the wire contract.
pub(super) fn delivery_settlement(
    record: &RunRecord,
) -> Option<peritus_run_settlement::RunSettlement> {
    record.settlement.filter(|settlement| {
        settlement.checkpoint().is_none() || record.snapshot.deliverable().is_some()
    })
}

pub(super) fn live_snapshot(
    record: &RunRecord,
) -> Result<ProductRunSnapshot, ProductRunServiceError> {
    if record.snapshot.phase().terminal() {
        return Ok(record.snapshot.clone());
    }
    replace_snapshot(
        &record.snapshot,
        record.snapshot.phase(),
        &record.progress.live_status(record.snapshot.status()),
        record.snapshot.summary(),
    )
}

pub(super) fn initial_snapshot(
    request: &ProductRunRequest,
) -> Result<ProductRunSnapshot, ProductRunServiceError> {
    ProductRunSnapshot::new(
        request.run_id(),
        request.workspace_id(),
        request.providers(),
        ProductRunPhase::Queued,
        1,
        request.task().to_owned(),
        "Queued for the writer".to_owned(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    )
    .map_err(|_| ProductRunServiceError::InvalidMessage)
}

pub(super) fn replace_snapshot(
    current: &ProductRunSnapshot,
    phase: ProductRunPhase,
    status: &str,
    summary: &str,
) -> Result<ProductRunSnapshot, ProductRunServiceError> {
    let snapshot = ProductRunSnapshot::new(
        current.run_id(),
        current.workspace_id(),
        current.providers(),
        phase,
        current.cycle(),
        current.task().to_owned(),
        status.to_owned(),
        current.diff().to_owned(),
        current.gates().to_owned(),
        current.review().to_owned(),
        summary.to_owned(),
    )
    .map_err(|_| ProductRunServiceError::InvalidMessage)?;
    Ok(match current.deliverable().cloned() {
        Some(deliverable) => snapshot.with_deliverable(deliverable),
        None => snapshot,
    })
}

pub(super) fn workspace_has_active_run(
    records: &BTreeMap<RunId, RunRecord>,
    workspace_id: WorkspaceId,
    except: Option<RunId>,
) -> bool {
    records.iter().any(|(run_id, record)| {
        if Some(*run_id) == except || record.request.workspace_id() != workspace_id {
            return false;
        }
        let active = !matches!(
            record.snapshot.phase(),
            ProductRunPhase::Complete
                | ProductRunPhase::Failed
                | ProductRunPhase::Cancelled
                | ProductRunPhase::WaitingForUser
                | ProductRunPhase::RecoveryRequired
        );
        let pending_handoff = record.snapshot.phase() == ProductRunPhase::Complete
            && record.snapshot.deliverable().is_some_and(|deliverable| {
                deliverable.commit_revision().is_empty() && !deliverable.discarded()
            });
        active || pending_handoff
    })
}
