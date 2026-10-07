//! Product-run snapshot construction and state replacement.

use std::collections::BTreeMap;

use peritus_app_protocol::{
    AppResponsePayload, ProductRunObservation, ProductRunPage, ProductRunPageEntry,
    ProductRunPageQuery, ProductRunPhase, ProductRunSettlementSnapshot, ProductRunSnapshot,
    ProductRunStoreId,
};
use peritus_types::{RunId, WorkspaceId};

use super::{ProductRunRequest, ProductRunServiceError, RunRecord};

impl super::ProductRunService {
    pub(crate) fn query_run_page(
        &self,
        query: ProductRunPageQuery,
    ) -> Result<ProductRunPage, ProductRunServiceError> {
        let catalog = self.inner.model_catalogs.runs()?;
        let store = ProductRunStoreId::new(*self.inner.control_store.as_bytes())
            .map_err(|_| ProductRunServiceError::InvalidState)?;
        let selected = catalog.select(query, store)?;
        drop(catalog);
        let captured = {
            let records =
                self.inner.records.read().map_err(|_| ProductRunServiceError::Unavailable)?;
            selected
                .keys
                .into_iter()
                .map(|key| {
                    records
                        .get(&key.run())
                        .cloned()
                        .map(|record| (key, record))
                        .ok_or(ProductRunServiceError::InvalidState)
                })
                .collect::<Result<Vec<_>, _>>()?
        };
        let entries = captured
            .into_iter()
            .map(|(key, record)| {
                ProductRunPageEntry::new(
                    key.sequence(),
                    observation(&self.inner.directory, &record)?,
                )
                .map_err(|_| ProductRunServiceError::InvalidState)
            })
            .collect::<Result<Vec<_>, _>>()?;
        ProductRunPage::new(store, entries, selected.next)
            .map_err(|_| ProductRunServiceError::InvalidState)
    }

    pub(crate) fn query_observations(
        &self,
        query: peritus_app_protocol::ProductRunQuery,
    ) -> Result<Vec<ProductRunObservation>, ProductRunServiceError> {
        if let Some(run) = query.run_id() {
            let record = self
                .inner
                .records
                .read()
                .map_err(|_| ProductRunServiceError::Unavailable)?
                .get(&run)
                .cloned();
            return record
                .as_ref()
                .map(|record| observation(&self.inner.directory, record))
                .transpose()
                .map(|value| value.into_iter().collect());
        }
        let records = self
            .inner
            .records
            .read()
            .map_err(|_| ProductRunServiceError::Unavailable)?;
        let captured = super::recent_records(&records, query.offset())
            .into_iter()
            .take(peritus_app_protocol::MAX_PRODUCT_RUN_PAGE)
            .cloned()
            .collect::<Vec<_>>();
        drop(records);
        captured
            .iter()
            .map(|record| observation(&self.inner.directory, record))
            .collect()
    }
}

fn observation(
    directory: &std::path::Path,
    record: &RunRecord,
) -> Result<ProductRunObservation, ProductRunServiceError> {
    ProductRunObservation::new(live_snapshot(directory, record)?, delivery_settlement(record))
        .map_err(|_| ProductRunServiceError::InvalidState)
}

pub(super) fn project_snapshot(
    directory: &std::path::Path,
    record: &RunRecord,
    snapshot: ProductRunSnapshot,
) -> Result<AppResponsePayload, ProductRunServiceError> {
    let snapshot = snapshot.with_operation(super::operation::project(directory, record)?);
    match delivery_settlement(record) {
        Some(settlement) => ProductRunSettlementSnapshot::new(snapshot, settlement)
            .map(AppResponsePayload::ProductRunSettled)
            .map_err(|_| ProductRunServiceError::InvalidState),
        None => Ok(AppResponsePayload::ProductRunAccepted(snapshot)),
    }
}

/// Managed candidates expose their exact settlement with the corresponding handoff snapshot.
/// In-place execution retains its checkpoint internally without inventing a Git handoff.
pub(super) fn delivery_settlement(
    record: &RunRecord,
) -> Option<peritus_run_settlement::RunSettlement> {
    record.settlement.filter(|settlement| {
        settlement.checkpoint().is_none() || record.snapshot.deliverable().is_some()
    })
}

pub(super) fn live_snapshot(
    directory: &std::path::Path,
    record: &RunRecord,
) -> Result<ProductRunSnapshot, ProductRunServiceError> {
    let snapshot = if record.snapshot.phase().terminal() {
        record.snapshot.clone()
    } else {
        replace_snapshot(
            &record.snapshot,
            record.snapshot.phase(),
            &record.progress.live_status(record.snapshot.status()),
            record.snapshot.summary(),
        )?
    };
    Ok(snapshot.with_operation(super::operation::project(directory, record)?))
}

pub(super) fn initial_snapshot(
    request: &ProductRunRequest,
) -> Result<ProductRunSnapshot, ProductRunServiceError> {
    queued_snapshot(request, "Queued to inspect the workspace and prepare the design")
}

pub(super) fn retry_snapshot(
    request: &ProductRunRequest,
    resume: Option<&peritus_product_runner::ProductRunResume>,
) -> Result<ProductRunSnapshot, ProductRunServiceError> {
    let status = match resume.map(peritus_product_runner::ProductRunResume::next_phase) {
        Some(peritus_product_runner::ProductRunPhase::Designing) => {
            "Queued to refresh the design for the current request"
        }
        Some(peritus_product_runner::ProductRunPhase::Writing) => "Queued to resume implementation",
        Some(peritus_product_runner::ProductRunPhase::Checking) => {
            "Queued to reacquire stale checks"
        }
        Some(peritus_product_runner::ProductRunPhase::Reviewing) => {
            "Queued to reacquire independent review"
        }
        Some(peritus_product_runner::ProductRunPhase::Fixing) => "Queued to resume fixes",
        Some(
            peritus_product_runner::ProductRunPhase::Verifying
            | peritus_product_runner::ProductRunPhase::Finalizing
            | peritus_product_runner::ProductRunPhase::Complete,
        ) => "Queued to refresh terminal evidence",
        None => "Queued to inspect the workspace and prepare a fresh design",
    };
    queued_snapshot(request, status)
}

fn queued_snapshot(
    request: &ProductRunRequest,
    status: &str,
) -> Result<ProductRunSnapshot, ProductRunServiceError> {
    ProductRunSnapshot::new(
        request.run_id(),
        request.workspace_id(),
        request.providers(),
        ProductRunPhase::Queued,
        1,
        request.display_task().to_owned(),
        status.to_owned(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        super::operation::retained_execution(request.run_id(), ProductRunPhase::Queued, "")?,
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
        super::operation::retained_execution(
            current.run_id(),
            phase,
            current.operation().uncertainty(),
        )?,
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
