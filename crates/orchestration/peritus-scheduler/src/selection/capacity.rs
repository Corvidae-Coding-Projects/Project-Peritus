//! Exact checked reservation sums and prospective capacity predicates.

use crate::{ResourceEntry, ResourceVector, SchedulerReservation, SchedulerState};

mod aggregate;
mod fallback;
mod refinement;

fn worker_count_from(
    reservations: &[SchedulerReservation],
    index: usize,
    worker: crate::WorkerId,
) -> usize {
    let mut cursor = reservations.len();
    let mut result = 0;
    while index < cursor {
        cursor -= 1;
        if reservations[cursor].worker_id().same(&worker) {
            result += 1;
        }
    }
    result
}
fn aggregate_fits_from(
    usage: &aggregate::Aggregate,
    entries: &[ResourceEntry],
    index: usize,
    capacity: &ResourceVector,
) -> bool {
    if index == entries.len() {
        return true;
    }
    let entry = entries[index];
    let used = u128::from(usage.quantity(entry.kind()));
    let requested = u128::from(entry.quantity().get());
    let available = u128::from(capacity.quantity(entry.kind()));
    let rest = aggregate_fits_from(usage, entries, index + 1, capacity);
    used + requested <= available && rest
}
fn global_fits_with_aggregate(
    state: &SchedulerState,
    request: &ResourceVector,
    usage: &aggregate::Aggregate,
) -> bool {
    aggregate_fits_from(usage, request.entries(), 0, state.binding().capacity())
}
fn worker_fits_with_aggregate(
    _state: &SchedulerState,
    _worker: crate::WorkerId,
    capacity: &ResourceVector,
    request: &ResourceVector,
    usage: &aggregate::Aggregate,
) -> bool {
    aggregate_fits_from(usage, request.entries(), 0, capacity)
}
fn global_fits_with_fallback(
    state: &SchedulerState,
    request: &ResourceVector,
    usage: &aggregate::Aggregate,
) -> bool {
    match usage {
        aggregate::Aggregate::Failed => {
            let entries = request.entries();

            fallback::global_fits_from(state, entries, 0)
        }
        aggregate::Aggregate::Empty | aggregate::Aggregate::Value(_) => {
            global_fits_with_aggregate(state, request, usage)
        }
    }
}
fn worker_fits_with_fallback(
    state: &SchedulerState,
    worker: crate::WorkerId,
    capacity: &ResourceVector,
    request: &ResourceVector,
    usage: &aggregate::Aggregate,
) -> bool {
    match usage {
        aggregate::Aggregate::Failed => {
            let entries = request.entries();

            fallback::worker_fits_from(state, worker, capacity, entries, 0)
        }
        aggregate::Aggregate::Empty | aggregate::Aggregate::Value(_) => {
            worker_fits_with_aggregate(state, worker, capacity, request, usage)
        }
    }
}
pub(super) fn global_fits_after(state: &SchedulerState, request: &ResourceVector) -> bool {
    if state.reservations().len() > 4_096 {
        false
    } else {
        let usage = aggregate::global(state.reservations());
        global_fits_with_fallback(state, request, &usage)
    }
}
pub(super) fn worker_fits_after(
    state: &SchedulerState,
    worker: crate::WorkerId,
    capacity: &ResourceVector,
    request: &ResourceVector,
) -> bool {
    if state.reservations().len() > 4_096 {
        false
    } else {
        let usage = aggregate::worker(state.reservations(), worker);
        worker_fits_with_fallback(state, worker, capacity, request, &usage)
    }
}
pub fn worker_reservation_count(state: &SchedulerState, worker: crate::WorkerId) -> usize {
    worker_count_from(state.reservations(), 0, worker)
}
