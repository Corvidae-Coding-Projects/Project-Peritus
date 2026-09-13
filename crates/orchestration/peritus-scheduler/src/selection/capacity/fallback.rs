//! Exact per-dimension fallback when canonical aggregate storage cannot represent a sum.

use vstd::prelude::*;

use crate::{ResourceEntry, ResourceKind, ResourceVector, SchedulerReservation, SchedulerState};

verus! {

fn global_quantity_from(
    reservations: &[SchedulerReservation],
    index: usize,
    kind: ResourceKind,
) -> (result: u128)
    requires
        index <= reservations@.len(),
        reservations@.len() <= 4_096,
    ensures
        result as int
            == crate::verified::reservation_quantity(
                reservations@.subrange(index as int, reservations@.len() as int),
                kind,
            ),
        result as int
            <= (reservations@.len() - index) * (u64::MAX as int),
    decreases reservations@.len() - index,
{
    if index == reservations.len() {
        proof {
            assert(reservations@.subrange(index as int, reservations@.len() as int).len() == 0);
        }
        0
    } else {
        let ghost suffix = reservations@.subrange(index as int, reservations@.len() as int);
        assert(suffix.len() > 0);
        assert(suffix.first() == reservations@[index as int]);
        assert(suffix.drop_first()
            =~= reservations@.subrange(index as int + 1, reservations@.len() as int));
        let head = u128::from(reservations[index].resources().quantity(kind));
        let tail = global_quantity_from(reservations, index + 1, kind);
        proof {
            reservations@[index as int].spec_resources().quantity_matches_entries(kind);
        }
        reveal(crate::verified::vector_quantity);
        assert(head as int == crate::verified::vector_quantity(
            suffix.first().spec_resources().spec_entries(),
            kind,
        ));
        assert(tail as int == crate::verified::reservation_quantity(suffix.drop_first(), kind));
        assert(head as int <= u64::MAX as int);
        assert(tail as int <= 4_095 * (u64::MAX as int));
        head + tail
    }
}

fn worker_quantity_from(
    reservations: &[SchedulerReservation],
    index: usize,
    worker: crate::WorkerId,
    kind: ResourceKind,
) -> (result: u128)
    requires
        index <= reservations@.len(),
        reservations@.len() <= 4_096,
    ensures
        result as int
            == crate::verified::worker_quantity(
                reservations@.subrange(index as int, reservations@.len() as int),
                worker,
                kind,
            ),
        result as int
            <= (reservations@.len() - index) * (u64::MAX as int),
    decreases reservations@.len() - index,
{
    if index == reservations.len() {
        proof {
            assert(reservations@.subrange(index as int, reservations@.len() as int).len() == 0);
        }
        0
    } else {
        let ghost suffix = reservations@.subrange(index as int, reservations@.len() as int);
        assert(suffix.len() > 0);
        assert(suffix.first() == reservations@[index as int]);
        assert(suffix.drop_first()
            =~= reservations@.subrange(index as int + 1, reservations@.len() as int));
        let tail = worker_quantity_from(reservations, index + 1, worker, kind);
        if reservations[index].worker_id().same(&worker) {
            let head = u128::from(reservations[index].resources().quantity(kind));
            proof {
                reservations@[index as int].spec_resources().quantity_matches_entries(kind);
            }
            reveal(crate::verified::vector_quantity);
            assert(suffix.first().spec_worker_id() == worker);
            assert(head as int == crate::verified::vector_quantity(
                suffix.first().spec_resources().spec_entries(),
                kind,
            ));
            assert(tail as int == crate::verified::worker_quantity(
                suffix.drop_first(), worker, kind,
            ));
            assert(head as int <= u64::MAX as int);
            assert(tail as int <= 4_095 * (u64::MAX as int));
            head + tail
        } else {
            assert(suffix.first().spec_worker_id() != worker);
            assert(tail as int == crate::verified::worker_quantity(
                suffix.drop_first(), worker, kind,
            ));
            tail
        }
    }
}


pub(super) fn global_fits_from(
    state: &SchedulerState,
    entries: &[ResourceEntry],
    index: usize,
) -> (result: bool)
    requires
        index <= entries@.len(),
        state.spec_reservations().len() <= 4_096,
    ensures
        result
            == super::global_entries_fit_after(
                state,
                entries@.subrange(index as int, entries@.len() as int),
            ),
    decreases entries@.len() - index,
{
    if index == entries.len() {
        proof {
            assert(entries@.subrange(index as int, entries@.len() as int).len() == 0);
        }
        true
    } else {
        let ghost suffix = entries@.subrange(index as int, entries@.len() as int);
        assert(suffix.len() > 0);
        assert(suffix.first() == entries@[index as int]);
        assert(suffix.drop_first()
            =~= entries@.subrange(index as int + 1, entries@.len() as int));
        let entry = entries[index];
        let used = global_quantity_from(state.reservations(), 0, entry.kind());
        let request = u128::from(entry.quantity().get());
        let capacity = u128::from(state.binding().capacity().quantity(entry.kind()));
        let rest = global_fits_from(state, entries, index + 1);
        assert(state.spec_reservations().subrange(
            0, state.spec_reservations().len() as int,
        ) =~= state.spec_reservations());
        assert(used as int == crate::verified::reservation_quantity(
            state.spec_reservations(), entry.spec_kind(),
        ));
        assert(capacity as int == state.spec_binding().spec_capacity()
            .spec_quantity(entry.spec_kind()));
        assert(rest == super::global_entries_fit_after(state, suffix.drop_first()));
        used + request <= capacity && rest
    }
}


pub(super) fn worker_fits_from(
    state: &SchedulerState,
    worker: crate::WorkerId,
    capacity: &ResourceVector,
    entries: &[ResourceEntry],
    index: usize,
) -> (result: bool)
    requires
        index <= entries@.len(),
        state.spec_reservations().len() <= 4_096,
    ensures
        result
            == super::worker_entries_fit_after(
                state,
                worker,
                capacity,
                entries@.subrange(index as int, entries@.len() as int),
            ),
    decreases entries@.len() - index,
{
    if index == entries.len() {
        proof {
            assert(entries@.subrange(index as int, entries@.len() as int).len() == 0);
        }
        true
    } else {
        let ghost suffix = entries@.subrange(index as int, entries@.len() as int);
        assert(suffix.len() > 0);
        assert(suffix.first() == entries@[index as int]);
        assert(suffix.drop_first()
            =~= entries@.subrange(index as int + 1, entries@.len() as int));
        let entry = entries[index];
        let used = worker_quantity_from(state.reservations(), 0, worker, entry.kind());
        let request = u128::from(entry.quantity().get());
        let available = u128::from(capacity.quantity(entry.kind()));
        let rest = worker_fits_from(state, worker, capacity, entries, index + 1);
        assert(state.spec_reservations().subrange(
            0, state.spec_reservations().len() as int,
        ) =~= state.spec_reservations());
        assert(used as int == crate::verified::worker_quantity(
            state.spec_reservations(), worker, entry.spec_kind(),
        ));
        assert(available as int == capacity.spec_quantity(entry.spec_kind()));
        assert(rest == super::worker_entries_fit_after(
            state, worker, capacity, suffix.drop_first(),
        ));
        used + request <= available && rest
    }
}

} // verus!
