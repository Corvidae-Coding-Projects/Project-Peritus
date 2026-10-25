//! Exact cancellation lifecycle updates for a preselected work sequence.

use crate::state::mutation;

use crate::{SchedulerState, WorkId, WorkPhase, WorkTerminal};

mod activity;

use activity::is_active;

#[doc = " Applies the reservation-sensitive lifecycle mutation for one selected identity."]
fn update_one(state: &mut SchedulerState, id: WorkId) -> bool {
    let active = is_active(state.reservations(), id);

    if active {
        mutation::set_work_phase(state, id, WorkPhase::Cancelling)
    } else {
        mutation::terminalize_work(state, id, WorkTerminal::Cancelled)
    }
}
#[doc = " Applies cancellation to each already-selected identity in order."]
#[doc = ""]
#[doc = " `false` means the first missing identity was encountered after applying the exact successful"]
#[doc = " prefix, matching the former inline reducer loop. Callers need no proof preconditions. When all"]
#[doc = " supplied identities came from retained work, the result is necessarily `true`."]
pub(super) fn apply_updates(state: &mut SchedulerState, affected: &[WorkId]) -> bool {
    let mut index = 0;
    while index < affected.len() {
        let id = affected[index];
        let updated = update_one(state, id);
        if !updated {
            return false;
        }
        index += 1;
    }
    true
}
