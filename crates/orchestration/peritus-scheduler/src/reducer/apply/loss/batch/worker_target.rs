//! Exact worker lookup and final lifecycle-target facts for loss application.

use crate::{SchedulerState, WorkerId, WorkerPhase};

use super::WorkerLossError;

pub(super) fn validate_worker_target(
    state: &SchedulerState,
    worker_id: WorkerId,
) -> Result<(), WorkerLossError> {
    let Some(worker) = state.worker(worker_id) else {
        return Err(WorkerLossError::WorkerMissing);
    };
    let phase = worker.phase();
    if matches!(phase, WorkerPhase::Lost | WorkerPhase::Removed) {
        return Err(WorkerLossError::AlreadyLostOrRemoved);
    }
    Ok(())
}
