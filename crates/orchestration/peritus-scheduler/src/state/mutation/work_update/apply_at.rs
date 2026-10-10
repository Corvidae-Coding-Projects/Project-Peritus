//! Exact lifecycle mutation at an already-validated retained work index.

use super::WorkUpdate;

use crate::{SchedulerState, WorkId};

pub(super) fn update_work_at(
    state: &mut SchedulerState,
    index: usize,
    _id: WorkId,
    update: WorkUpdate,
) {
    match update {
        WorkUpdate::Phase(phase) => state.work[index].set_phase(phase),
        WorkUpdate::QueueRetry => state.work[index].queue_retry(),
        WorkUpdate::RetryPending(cause) => state.work[index].set_retry_pending(cause),
        WorkUpdate::Terminal(terminal) => state.work[index].terminalize(terminal),
    }
}
