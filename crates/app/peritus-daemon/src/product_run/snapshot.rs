//! Product-run snapshot construction and state replacement.

use std::collections::BTreeMap;

use peritus_app_protocol::{
    AppResponsePayload, ProductRunPhase, ProductRunRequest, ProductRunSettlementSnapshot,
    ProductRunSnapshot,
};
use peritus_types::{RunId, WorkspaceId};

use super::{ProductRunServiceError, RunRecord};

pub(super) fn project_snapshot(
    record: &RunRecord,
    snapshot: ProductRunSnapshot,
) -> Result<AppResponsePayload, ProductRunServiceError> {
    match record.settlement {
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
                .and_then(|record| record.settlement)
                .map(|settlement| ProductRunSettlementSnapshot::new(snapshot.clone(), settlement))
                .transpose()
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| ProductRunServiceError::InvalidState)?;
    if settled.iter().all(Option::is_some) {
        Ok(AppResponsePayload::ProductRunSettlements(settled.into_iter().flatten().collect()))
    } else {
        Ok(AppResponsePayload::ProductRuns(snapshots))
    }
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
                | ProductRunPhase::RecoveryRequired
        );
        let pending_handoff = record.snapshot.phase() == ProductRunPhase::Complete
            && record.snapshot.deliverable().is_some_and(|deliverable| {
                deliverable.commit_revision().is_empty() && !deliverable.discarded()
            });
        active || pending_handoff
    })
}
