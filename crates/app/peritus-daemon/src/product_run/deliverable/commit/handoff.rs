//! Bind a successful explicit commit without transferring qualification across Git identities.

use super::{ProductDeliverable, ProductRunServiceError, attempt};
use crate::product_run::{RunRecord, recovery};
use peritus_product_runner::ProductRunner;
use peritus_run_settlement::{SettlementCause, SettlementReducer};
use std::path::Path;

pub(super) fn bind(
    directory: &Path,
    record: &mut RunRecord,
    deliverable: ProductDeliverable,
) -> Result<(ProductDeliverable, &'static str), ProductRunServiceError> {
    let Some(previous) = record.checkpoint else {
        return Ok((deliverable, ""));
    };
    // Git hooks may have changed source after the commit. Keep the actual commit
    // and saved patch visible, but never adopt those edits as the settled candidate.
    if !attempt::matches(directory, record, &deliverable)? {
        return Ok((
            deliverable,
            "; workspace changed during commit; continue to inspect and requalify it",
        ));
    }
    let current = ProductRunner::candidate_digest(Path::new(deliverable.workspace_path()))
        .map_err(|_| ProductRunServiceError::WorkspaceUnavailable)?;
    if !attempt::matches(directory, record, &deliverable)? {
        return Ok((
            deliverable,
            "; workspace changed during commit; continue to inspect and requalify it",
        ));
    }
    if current == previous.identity().candidate_digest() {
        return Ok((deliverable, ""));
    }
    let checkpoint = recovery::changed_checkpoint(previous, current)?;
    let cause = record.settlement.map_or(SettlementCause::Completed, |value| value.cause());
    let mut reducer = SettlementReducer::new();
    reducer.observe(previous).map_err(|_| ProductRunServiceError::InvalidState)?;
    reducer.observe(checkpoint).map_err(|_| ProductRunServiceError::InvalidState)?;
    let settlement = reducer.settle(cause).map_err(|_| ProductRunServiceError::InvalidState)?;
    let current_deliverable = ProductDeliverable::candidate(
        deliverable.workspace_path().to_owned(),
        deliverable.changed_paths().to_vec(),
        deliverable.successful_commands().to_vec(),
        deliverable.run_instructions().to_owned(),
        checkpoint.stage(),
    )
    .and_then(|value| value.mark_exported(deliverable.export_path().to_owned()))
    .and_then(|value| value.mark_committed(deliverable.commit_revision().to_owned()))
    .map_err(|_| ProductRunServiceError::InvalidMessage)?;
    record.checkpoint = Some(checkpoint);
    record.settlement = Some(settlement);
    record.resume = None;
    record.candidate_actionable = true;
    Ok((
        current_deliverable,
        "; Run is available; pre-commit checks and review remain historical evidence",
    ))
}
