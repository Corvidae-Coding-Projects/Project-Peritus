//! Exact per-dimension fallback when canonical aggregate storage cannot represent a sum.

use crate::{ResourceEntry, ResourceKind, ResourceVector, SchedulerReservation, SchedulerState};

fn global_quantity_from(
    reservations: &[SchedulerReservation],
    index: usize,
    kind: ResourceKind,
) -> u128 {
    if index == reservations.len() {
        0
    } else {
        let head = u128::from(reservations[index].resources().quantity(kind));
        let tail = global_quantity_from(reservations, index + 1, kind);
        head + tail
    }
}
fn worker_quantity_from(
    reservations: &[SchedulerReservation],
    index: usize,
    worker: crate::WorkerId,
    kind: ResourceKind,
) -> u128 {
    if index == reservations.len() {
        0
    } else {
        let tail = worker_quantity_from(reservations, index + 1, worker, kind);
        if reservations[index].worker_id().same(&worker) {
            let head = u128::from(reservations[index].resources().quantity(kind));
            head + tail
        } else {
            tail
        }
    }
}
pub(super) fn global_fits_from(
    state: &SchedulerState,
    entries: &[ResourceEntry],
    index: usize,
) -> bool {
    if index == entries.len() {
        true
    } else {
        let entry = entries[index];
        let used = global_quantity_from(state.reservations(), 0, entry.kind());
        let request = u128::from(entry.quantity().get());
        let capacity = u128::from(state.binding().capacity().quantity(entry.kind()));
        let rest = global_fits_from(state, entries, index + 1);
        used + request <= capacity && rest
    }
}
pub(super) fn worker_fits_from(
    state: &SchedulerState,
    worker: crate::WorkerId,
    capacity: &ResourceVector,
    entries: &[ResourceEntry],
    index: usize,
) -> bool {
    if index == entries.len() {
        true
    } else {
        let entry = entries[index];
        let used = worker_quantity_from(state.reservations(), 0, worker, entry.kind());
        let request = u128::from(entry.quantity().get());
        let available = u128::from(capacity.quantity(entry.kind()));
        let rest = worker_fits_from(state, worker, capacity, entries, index + 1);
        used + request <= available && rest
    }
}
