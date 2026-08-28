//! Product-run snapshot construction and state replacement.

use peritus_app_protocol::{ProductRunPhase, ProductRunRequest, ProductRunSnapshot};

use super::ProductRunServiceError;

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
