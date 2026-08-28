//! Product-run snapshot construction and state replacement.

use std::collections::BTreeMap;

use peritus_app_protocol::{ProductRunPhase, ProductRunRequest, ProductRunSnapshot};
use peritus_types::{RunId, WorkspaceId};

use super::{ProductRunServiceError, RunRecord};

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
    ProductRunSnapshot::new(
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
    .map_err(|_| ProductRunServiceError::InvalidMessage)
}

pub(super) fn workspace_has_active_run(
    records: &BTreeMap<RunId, RunRecord>,
    workspace_id: WorkspaceId,
    except: Option<RunId>,
) -> bool {
    records.iter().any(|(run_id, record)| {
        Some(*run_id) != except
            && record.request.workspace_id() == workspace_id
            && !matches!(
                record.snapshot.phase(),
                ProductRunPhase::Complete
                    | ProductRunPhase::Failed
                    | ProductRunPhase::Cancelled
                    | ProductRunPhase::RecoveryRequired
            )
    })
}
