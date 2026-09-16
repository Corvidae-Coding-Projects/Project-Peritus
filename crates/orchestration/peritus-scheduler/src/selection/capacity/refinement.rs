//! Refinement from finite request-entry checks to all resource kinds.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::{ResourceEntry, ResourceKind, ResourceVector, SchedulerState};

#[cfg(verus_only)]
use super::{global_entries_fit_after, worker_entries_fit_after};

verus! {

proof fn quantity_has_entry(entries: Seq<ResourceEntry>, kind: ResourceKind)
    ensures
        crate::resource::capacity::entries_quantity(entries, kind) == 0
            || exists |index: int| #![auto]
                0 <= index < entries.len()
                    && entries[index].spec_kind() == kind
                    && crate::resource::capacity::entries_quantity(entries, kind)
                        == entries[index].spec_quantity().spec_value(),
    decreases entries.len(),
{
    if entries.len() > 0 {
        if entries.first().spec_kind() == kind {
            if entries.first().spec_quantity().spec_value() != 0 {
                assert(exists |index: int| #![auto]
                    0 <= index < entries.len()
                        && entries[index].spec_kind() == kind
                        && crate::resource::capacity::entries_quantity(entries, kind)
                            == entries[index].spec_quantity().spec_value());
            }
        } else {
            quantity_has_entry(entries.drop_first(), kind);
            if crate::resource::capacity::entries_quantity(entries.drop_first(), kind) != 0 {
                let tail_index = choose |index: int| #![auto]
                    0 <= index < entries.drop_first().len()
                        && entries.drop_first()[index].spec_kind() == kind
                        && crate::resource::capacity::entries_quantity(entries.drop_first(), kind)
                            == entries.drop_first()[index].spec_quantity().spec_value();
                assert(entries.drop_first()[tail_index] == entries[tail_index + 1]);
                assert(exists |index: int| #![auto]
                    0 <= index < entries.len()
                        && entries[index].spec_kind() == kind
                        && crate::resource::capacity::entries_quantity(entries, kind)
                            == entries[index].spec_quantity().spec_value());
            }
        }
    }
}

proof fn global_bound_at(state: &SchedulerState, entries: Seq<ResourceEntry>, index: int)
    requires
        global_entries_fit_after(state, entries),
        0 <= index < entries.len(),
    ensures
        crate::verified::reservation_quantity(
            state.spec_reservations(),
            entries[index].spec_kind(),
        ) + entries[index].spec_quantity().spec_value()
            <= state.spec_binding().spec_capacity().spec_quantity(entries[index].spec_kind()),
    decreases index,
{
    if index > 0 {
        global_bound_at(state, entries.drop_first(), index - 1);
        assert(entries.drop_first()[index - 1] == entries[index]);
    }
}

proof fn worker_bound_at(
    state: &SchedulerState,
    worker: crate::WorkerId,
    capacity: &ResourceVector,
    entries: Seq<ResourceEntry>,
    index: int,
)
    requires
        worker_entries_fit_after(state, worker, capacity, entries),
        0 <= index < entries.len(),
    ensures
        crate::verified::worker_quantity(
            state.spec_reservations(),
            worker,
            entries[index].spec_kind(),
        ) + entries[index].spec_quantity().spec_value()
            <= capacity.spec_quantity(entries[index].spec_kind()),
    decreases index,
{
    if index > 0 {
        worker_bound_at(state, worker, capacity, entries.drop_first(), index - 1);
        assert(entries.drop_first()[index - 1] == entries[index]);
    }
}

pub(crate) proof fn global_entrywise_implies_all(
    state: &SchedulerState,
    request: &ResourceVector,
)
    requires
        state.spec_reservation_invariant(),
        global_entries_fit_after(state, request.spec_entries()),
    ensures
        forall |kind: ResourceKind|
            crate::verified::reservation_quantity(state.spec_reservations(), kind)
                    + crate::verified::vector_quantity(request.spec_entries(), kind)
                <= state.spec_binding().spec_capacity().spec_quantity(kind),
{
    reveal(crate::verified::reservation_invariant_parts);
    reveal(crate::verified::vector_quantity);
    assert forall |kind: ResourceKind| #![auto]
        crate::verified::reservation_quantity(state.spec_reservations(), kind)
                + crate::verified::vector_quantity(request.spec_entries(), kind)
            <= state.spec_binding().spec_capacity().spec_quantity(kind) by {
        request.quantity_matches_entries(kind);
        state.spec_binding().spec_capacity().quantity_matches_entries(kind);
        quantity_has_entry(request.spec_entries(), kind);
        if crate::verified::vector_quantity(request.spec_entries(), kind) != 0 {
            let index = choose |index: int| #![auto]
                0 <= index < request.spec_entries().len()
                    && request.spec_entries()[index].spec_kind() == kind
                    && crate::resource::capacity::entries_quantity(
                        request.spec_entries(), kind,
                    ) == request.spec_entries()[index].spec_quantity().spec_value();
            global_bound_at(state, request.spec_entries(), index);
            assert(crate::verified::vector_quantity(request.spec_entries(), kind)
                == request.spec_entries()[index].spec_quantity().spec_value());
        } else {
            assert(crate::verified::reservation_quantity(state.spec_reservations(), kind)
                <= crate::verified::vector_quantity(
                    state.spec_binding().spec_capacity().spec_entries(), kind,
                ));
        }
    }
}

pub(crate) proof fn worker_entrywise_implies_all(
    state: &SchedulerState,
    worker_index: int,
    request: &ResourceVector,
)
    requires
        state.spec_reservation_invariant(),
        0 <= worker_index < state.spec_workers().len(),
        worker_entries_fit_after(
            state,
            state.spec_workers()[worker_index].spec_descriptor().spec_id(),
            state.spec_workers()[worker_index].spec_descriptor().spec_capacity(),
            request.spec_entries(),
        ),
    ensures
        forall |kind: ResourceKind|
            crate::verified::worker_quantity(
                state.spec_reservations(),
                state.spec_workers()[worker_index].spec_descriptor().spec_id(),
                kind,
            ) + crate::verified::vector_quantity(request.spec_entries(), kind)
                <= state.spec_workers()[worker_index].spec_descriptor()
                    .spec_capacity().spec_quantity(kind),
{
    reveal(crate::verified::vector_quantity);
    assert forall |kind: ResourceKind| #![auto]
        crate::verified::worker_quantity(
            state.spec_reservations(),
            state.spec_workers()[worker_index].spec_descriptor().spec_id(),
            kind,
        ) + crate::verified::vector_quantity(request.spec_entries(), kind)
            <= state.spec_workers()[worker_index].spec_descriptor()
                .spec_capacity().spec_quantity(kind) by {
        request.quantity_matches_entries(kind);
        state.spec_workers()[worker_index].spec_descriptor()
            .spec_capacity().quantity_matches_entries(kind);
        quantity_has_entry(request.spec_entries(), kind);
        if crate::verified::vector_quantity(request.spec_entries(), kind) != 0 {
            let index = choose |index: int| #![auto]
                0 <= index < request.spec_entries().len()
                    && request.spec_entries()[index].spec_kind() == kind
                    && crate::resource::capacity::entries_quantity(
                        request.spec_entries(), kind,
                    ) == request.spec_entries()[index].spec_quantity().spec_value();
            worker_bound_at(
                state,
                state.spec_workers()[worker_index].spec_descriptor().spec_id(),
                state.spec_workers()[worker_index].spec_descriptor().spec_capacity(),
                request.spec_entries(),
                index,
            );
            assert(crate::verified::vector_quantity(request.spec_entries(), kind)
                == request.spec_entries()[index].spec_quantity().spec_value());
        } else {
            reveal(crate::verified::reservation_invariant_parts);
            assert(crate::verified::worker_quantity(
                state.spec_reservations(),
                state.spec_workers()[worker_index].spec_descriptor().spec_id(),
                kind,
            ) <= crate::verified::vector_quantity(
                state.spec_workers()[worker_index].spec_descriptor()
                    .spec_capacity().spec_entries(), kind,
            ));
        }
    }
}

} // verus!
