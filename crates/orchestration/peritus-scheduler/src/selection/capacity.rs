//! Exact checked reservation sums and prospective capacity predicates.

use vstd::prelude::*;

use crate::{ResourceEntry, ResourceVector, SchedulerReservation, SchedulerState};

mod aggregate;
mod fallback;
mod refinement;

#[cfg(verus_only)]
pub(super) use refinement::{global_entrywise_implies_all, worker_entrywise_implies_all};

verus! {

pub open spec fn global_entries_fit_after(
    state: &SchedulerState,
    entries: Seq<ResourceEntry>,
) -> bool
    decreases entries.len(),
{
    entries.len() == 0 || {
        let entry = entries.first();
        crate::verified::reservation_quantity(state.spec_reservations(), entry.spec_kind())
                + entry.spec_quantity().spec_value()
            <= state.spec_binding().spec_capacity().spec_quantity(entry.spec_kind())
            && global_entries_fit_after(state, entries.drop_first())
    }
}

pub open spec fn worker_entries_fit_after(
    state: &SchedulerState,
    worker: crate::WorkerId,
    capacity: &ResourceVector,
    entries: Seq<ResourceEntry>,
) -> bool
    decreases entries.len(),
{
    entries.len() == 0 || {
        let entry = entries.first();
        crate::verified::worker_quantity(
            state.spec_reservations(),
            worker,
            entry.spec_kind(),
        ) + entry.spec_quantity().spec_value()
            <= capacity.spec_quantity(entry.spec_kind())
            && worker_entries_fit_after(state, worker, capacity, entries.drop_first())
    }
}

fn worker_count_from(
    reservations: &[SchedulerReservation],
    index: usize,
    worker: crate::WorkerId,
) -> (result: usize)
    requires index <= reservations@.len(),
    ensures
        result as int
            == crate::verified::worker_count(
                reservations@.subrange(index as int, reservations@.len() as int),
                worker,
            ),
        result <= reservations.len() - index,
{
    let mut cursor = reservations.len();
    let mut result = 0;
    proof {
        assert(reservations@.subrange(cursor as int, reservations@.len() as int).len() == 0);
    }
    while index < cursor
        invariant
            index <= cursor <= reservations@.len(),
            result as int
                == crate::verified::worker_count(
                    reservations@.subrange(cursor as int, reservations@.len() as int),
                    worker,
                ),
            result <= reservations.len() - cursor,
        decreases cursor - index,
    {
        let ghost previous_cursor = cursor;
        cursor -= 1;
        let ghost suffix = reservations@.subrange(cursor as int, reservations@.len() as int);
        proof {
            assert(previous_cursor == cursor + 1);
            assert(suffix.len() > 0);
            assert(suffix.first() == reservations@[cursor as int]);
            assert(suffix.drop_first()
                =~= reservations@.subrange(previous_cursor as int, reservations@.len() as int));
        }
        if reservations[cursor].worker_id().same(&worker) {
            proof { assert(result < usize::MAX); }
            result += 1;
        }
        proof { reveal(crate::verified::worker_count); };
    }
    proof { assert(cursor == index); }
    result
}

closed spec fn aggregate_entries_fit_after(
    usage: &aggregate::Aggregate,
    entries: Seq<ResourceEntry>,
    capacity: &ResourceVector,
) -> bool
    decreases entries.len(),
{
    entries.len() == 0 || {
        let entry = entries.first();
        usage.spec_quantity(entry.spec_kind()) + entry.spec_quantity().spec_value()
            <= capacity.spec_quantity(entry.spec_kind())
            && aggregate_entries_fit_after(usage, entries.drop_first(), capacity)
    }
}

fn aggregate_fits_from(
    usage: &aggregate::Aggregate,
    entries: &[ResourceEntry],
    index: usize,
    capacity: &ResourceVector,
) -> (result: bool)
    requires
        index <= entries@.len(),
    ensures
        result == aggregate_entries_fit_after(
            usage,
            entries@.subrange(index as int, entries@.len() as int),
            capacity,
        ),
    decreases entries@.len() - index,
{
    reveal(aggregate_entries_fit_after);
    if index == entries.len() {
        assert(entries@.subrange(index as int, entries@.len() as int).len() == 0);
        return true;
    }
    let ghost suffix = entries@.subrange(index as int, entries@.len() as int);
    assert(suffix.len() > 0);
    assert(suffix.first() == entries@[index as int]);
    assert(suffix.drop_first()
        =~= entries@.subrange(index as int + 1, entries@.len() as int));
    let entry = entries[index];
    let used = u128::from(usage.quantity(entry.kind()));
    let requested = u128::from(entry.quantity().get());
    let available = u128::from(capacity.quantity(entry.kind()));
    let rest = aggregate_fits_from(usage, entries, index + 1, capacity);
    assert(rest == aggregate_entries_fit_after(usage, suffix.drop_first(), capacity));
    used + requested <= available && rest
}

proof fn aggregate_fit_matches_global(
    state: &SchedulerState,
    usage: &aggregate::Aggregate,
    entries: Seq<ResourceEntry>,
)
    requires
        usage.spec_usable(),
        aggregate::global_sound(usage, state.spec_reservations()),
    ensures aggregate_entries_fit_after(usage, entries, state.spec_binding().spec_capacity())
        == global_entries_fit_after(state, entries),
    decreases entries.len(),
{
    reveal(aggregate_entries_fit_after);
    if entries.len() > 0 {
        let entry = entries.first();
        usage.global_quantity_matches(state.spec_reservations(), entry.spec_kind());
        aggregate_fit_matches_global(state, usage, entries.drop_first());
    }
}

proof fn aggregate_fit_matches_worker(
    state: &SchedulerState,
    worker: crate::WorkerId,
    capacity: &ResourceVector,
    usage: &aggregate::Aggregate,
    entries: Seq<ResourceEntry>,
)
    requires
        usage.spec_usable(),
        aggregate::worker_sound(usage, state.spec_reservations(), worker),
    ensures aggregate_entries_fit_after(usage, entries, capacity)
        == worker_entries_fit_after(state, worker, capacity, entries),
    decreases entries.len(),
{
    reveal(aggregate_entries_fit_after);
    if entries.len() > 0 {
        let entry = entries.first();
        usage.worker_quantity_matches(
            state.spec_reservations(), worker, entry.spec_kind());
        aggregate_fit_matches_worker(state, worker, capacity, usage, entries.drop_first());
    }
}

fn global_fits_with_aggregate(
    state: &SchedulerState,
    request: &ResourceVector,
    usage: &aggregate::Aggregate,
) -> (result: bool)
    requires
        usage.spec_usable(),
        aggregate::global_sound(usage, state.spec_reservations()),
    ensures result == global_entries_fit_after(state, request.spec_entries()),
{
    let result = aggregate_fits_from(
        usage,
        request.entries(),
        0,
        state.binding().capacity(),
    );
    proof {
        aggregate_fit_matches_global(state, usage, request.spec_entries());
        assert(request.spec_entries().subrange(
            0, request.spec_entries().len() as int) =~= request.spec_entries());
    }
    result
}

fn worker_fits_with_aggregate(
    _state: &SchedulerState,
    _worker: crate::WorkerId,
    capacity: &ResourceVector,
    request: &ResourceVector,
    usage: &aggregate::Aggregate,
) -> (result: bool)
    requires
        usage.spec_usable(),
        aggregate::worker_sound(usage, _state.spec_reservations(), _worker),
    ensures result == worker_entries_fit_after(
        _state, _worker, capacity, request.spec_entries()),
{
    let result = aggregate_fits_from(usage, request.entries(), 0, capacity);
    proof {
        aggregate_fit_matches_worker(
            _state, _worker, capacity, usage, request.spec_entries());
        assert(request.spec_entries().subrange(
            0, request.spec_entries().len() as int) =~= request.spec_entries());
    }
    result
}

fn global_fits_with_fallback(
    state: &SchedulerState,
    request: &ResourceVector,
    usage: &aggregate::Aggregate,
) -> (result: bool)
    requires
        state.spec_reservations().len() <= 4_096,
        aggregate::global_sound(usage, state.spec_reservations()),
    ensures result == global_entries_fit_after(state, request.spec_entries()),
{
    match usage {
        aggregate::Aggregate::Failed => {
            let entries = request.entries();
            let result = fallback::global_fits_from(state, entries, 0);
            assert(entries@.subrange(0, entries@.len() as int) =~= entries@);
            result
        }
        aggregate::Aggregate::Empty | aggregate::Aggregate::Value(_) => {
            assert(usage.spec_usable());
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
) -> (result: bool)
    requires
        state.spec_reservations().len() <= 4_096,
        aggregate::worker_sound(usage, state.spec_reservations(), worker),
    ensures result == worker_entries_fit_after(
        state, worker, capacity, request.spec_entries()),
{
    match usage {
        aggregate::Aggregate::Failed => {
            let entries = request.entries();
            let result = fallback::worker_fits_from(
                state, worker, capacity, entries, 0);
            assert(entries@.subrange(0, entries@.len() as int) =~= entries@);
            result
        }
        aggregate::Aggregate::Empty | aggregate::Aggregate::Value(_) => {
            assert(usage.spec_usable());
            worker_fits_with_aggregate(state, worker, capacity, request, usage)
        }
    }
}

pub(super) fn global_fits_after(
    state: &SchedulerState,
    request: &ResourceVector,
) -> (result: bool)
    ensures
        state.spec_reservations().len() <= 4_096 ==>
            result == global_entries_fit_after(state, request.spec_entries()),
        state.spec_reservation_invariant() && result ==>
            global_entries_fit_after(state, request.spec_entries()),
{
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
) -> (result: bool)
    ensures
        state.spec_reservations().len() <= 4_096 ==>
            result == worker_entries_fit_after(state, worker, capacity, request.spec_entries()),
        state.spec_reservation_invariant() && result ==>
            worker_entries_fit_after(state, worker, capacity, request.spec_entries()),
{
    if state.reservations().len() > 4_096 {
        false
    } else {
        let usage = aggregate::worker(state.reservations(), worker);
        worker_fits_with_fallback(state, worker, capacity, request, &usage)
    }
}

pub fn worker_reservation_count(
    state: &SchedulerState,
    worker: crate::WorkerId,
) -> (result: usize)
    ensures
        state.spec_reservation_invariant() ==>
            result as int
                == crate::verified::worker_count(state.spec_reservations(), worker),
{
    let result = worker_count_from(state.reservations(), 0, worker);
    proof {
        assert(state.spec_reservations().subrange(
            0, state.spec_reservations().len() as int,
        ) =~= state.spec_reservations());
    }
    result
}

} // verus!
