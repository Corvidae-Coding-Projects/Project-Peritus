//! Verified preparation of one exact selected reservation.

use peritus_types::Sha256Digest;

use crate::{DispatchId, SchedulerReservation, SchedulerState};

pub fn selected_reservation(
    state: &mut SchedulerState,
    work_index: usize,
    worker_index: usize,
    dispatch_id: DispatchId,
    dispatch_token: Sha256Digest,
) -> Option<SchedulerReservation> {
    if work_index >= state.work().len() || worker_index >= state.workers().len() {
        return None;
    }
    let work = &state.work()[work_index];
    if work.attempts_started() == u16::MAX
        || work.attempts_started() >= work.spec().maximum_attempts().get()
        || state.dispatch_ordinal() == u64::MAX
    {
        return None;
    }
    let work_id = work.spec().id();
    let owner = work.spec().owner();
    let revision = work.spec().revision();
    let resources = work.spec().request().clone();
    let worker_id = state.workers()[worker_index].descriptor().id();
    let attempt = super::super::begin_work_attempt_at(state, work_index);
    let Some(attempt) = attempt else {
        return None;
    };
    let incremented = super::super::increment_dispatch_ordinal(state);
    if !incremented {
        return None;
    }
    let reservation = SchedulerReservation::new(
        work_id,
        dispatch_id,
        worker_id,
        owner,
        attempt,
        revision,
        resources,
        dispatch_token,
    );
    Some(reservation)
}
