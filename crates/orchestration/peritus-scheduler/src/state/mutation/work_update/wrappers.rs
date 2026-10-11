//! Public phase-update operations over the verified work mutation kernel.

use crate::{SchedulerState, WorkId, WorkPhase, WorkTerminal};

use super::{WorkUpdate, update_work};

pub fn set_work_phase(state: &mut SchedulerState, id: WorkId, phase: WorkPhase) -> bool {
    let found = update_work(state, id, WorkUpdate::Phase(phase));
    found
}
pub fn queue_work_retry(state: &mut SchedulerState, id: WorkId) -> bool {
    update_work(state, id, WorkUpdate::QueueRetry)
}
pub fn set_work_bypasses(state: &mut SchedulerState, id: WorkId, value: u16) -> bool {
    let mut index = 0;
    while index < state.work.len() {
        if state.work[index].spec().id() == id {
            state.work[index].set_bypasses(value);
            return true;
        }
        index += 1;
    }
    false
}
pub fn terminalize_work(state: &mut SchedulerState, id: WorkId, terminal: WorkTerminal) -> bool {
    let found = update_work(state, id, WorkUpdate::Terminal(terminal));
    found
}
