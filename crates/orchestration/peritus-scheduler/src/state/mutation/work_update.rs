//! Verified reservation-stable work lifecycle mutations.

use crate::{SchedulerState, WorkId, WorkPhase, WorkTerminal};
use peritus_types::Sha256Digest;

mod apply_at;
mod attempt;
mod release;
mod wrappers;

use apply_at::update_work_at;
pub use attempt::begin_work_attempt_at;

pub use release::{release_to_phase, release_to_retry_pending, release_to_terminal};
pub use wrappers::{queue_work_retry, set_work_bypasses, set_work_phase, terminalize_work};

pub(super) enum WorkUpdate {
    Phase(WorkPhase),
    QueueRetry,
    RetryPending(Sha256Digest),
    Terminal(WorkTerminal),
}
impl WorkUpdate {}
pub(super) fn update_work(state: &mut SchedulerState, id: WorkId, update: WorkUpdate) -> bool {
    let mut index = 0;
    while index < state.work.len() {
        if state.work[index].spec().id().same(&id) {
            update_work_at(state, index, id, update);
            return true;
        }
        index += 1;
    }
    false
}
