//! Exact work-attempt mutation used by reservation admission.

use crate::SchedulerState;

pub fn begin_work_attempt_at(
    state: &mut SchedulerState,
    index: usize,
) -> Option<crate::AttemptNumber> {
    if index >= state.work.len() {
        return None;
    }
    let result = state.work[index].try_begin_attempt();
    result
}
