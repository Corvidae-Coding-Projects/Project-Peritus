//! Verified production mutation that installs one selected reservation.

mod prepare;
mod proof;

use crate::{DispatchId, SchedulerReservation, SchedulerState};
use peritus_types::Sha256Digest;

#[doc = " Begins and durably binds one selector-admitted work/worker pair."]
pub fn reserve_selected_at(
    state: &mut SchedulerState,
    work_index: usize,
    worker_index: usize,
    dispatch_id: DispatchId,
    dispatch_token: Sha256Digest,
) -> Option<SchedulerReservation> {
    let prepared =
        prepare::selected_reservation(state, work_index, worker_index, dispatch_id, dispatch_token);
    let reservation = prepared?;
    let stored = reservation.clone();
    super::retain_dispatch_identity(state, dispatch_id);
    super::insert_reservation(state, stored);
    Some(reservation)
}
