//! Verified reservation-stable worker lifecycle mutations.

use vstd::prelude::*;

use crate::{SchedulerState, WorkerId, WorkerPhase};

verus! {

/// Exact worker-sequence effect of one phase update attempt.
pub open spec fn worker_phase_update_matches(
    before: Seq<crate::WorkerRecord>,
    after: Seq<crate::WorkerRecord>,
    id: WorkerId,
    phase: WorkerPhase,
    found: bool,
) -> bool {
    &&& before.len() == after.len()
    &&& if found {
        exists |index: int| #![trigger before[index]] {
            &&& 0 <= index < before.len()
            &&& before[index].spec_descriptor().spec_id() == id
            &&& crate::WorkerRecord::reservation_owner_equivalent(
                &before[index],
                &after[index],
            )
            &&& after[index].spec_descriptor() == before[index].spec_descriptor()
            &&& after[index].spec_phase() == phase
            &&& forall |other: int| #![auto]
                0 <= other < before.len() && other != index ==>
                    after[other] == before[other]
        }
    } else {
        &&& after == before
        &&& forall |index: int| #![trigger before[index]]
            0 <= index < before.len() ==>
                before[index].spec_descriptor().spec_id() != id
    }
}

/// Relates every scheduler field outside a worker lifecycle update.
pub open spec fn worker_update_preserves_other_state(
    before: &SchedulerState,
    after: &SchedulerState,
) -> bool {
    &&& after.spec_binding() == before.spec_binding()
    &&& after.spec_phase() == before.spec_phase()
    &&& after.spec_sequence() == before.spec_sequence()
    &&& after.spec_last_event_id() == before.spec_last_event_id()
    &&& after.spec_state_digest() == before.spec_state_digest()
    &&& after.spec_work() == before.spec_work()
    &&& after.spec_reservations() == before.spec_reservations()
    &&& after.spec_used_dispatches() == before.spec_used_dispatches()
    &&& after.spec_enqueue_ordinal() == before.spec_enqueue_ordinal()
    &&& after.spec_dispatch_ordinal() == before.spec_dispatch_ordinal()
    &&& after.spec_used_commands() == before.spec_used_commands()
    &&& after.spec_terminal() == before.spec_terminal()
}

/// Exact complete-state effect of one worker lifecycle update attempt.
pub open spec fn worker_phase_state_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    id: WorkerId,
    phase: WorkerPhase,
    found: bool,
) -> bool {
    &&& worker_phase_update_matches(
        before.spec_workers(), after.spec_workers(), id, phase, found,
    )
    &&& worker_update_preserves_other_state(before, after)
}

/// Worker lifecycle changes retain identity order for every authoritative collection.
proof fn worker_phase_state_preserves_collections_order(
    before: &SchedulerState,
    after: &SchedulerState,
    id: WorkerId,
    phase: WorkerPhase,
    found: bool,
)
    requires
        before.spec_collections_ordered(),
        worker_phase_state_matches(before, after, id, phase, found),
    ensures after.spec_collections_ordered(),
{
    reveal(worker_phase_state_matches);
    reveal(worker_update_preserves_other_state);
    reveal(worker_phase_update_matches);
    reveal(SchedulerState::spec_collections_ordered);
    reveal(SchedulerState::spec_workers_ordered);
    reveal(SchedulerState::worker_records_ordered);
    if found {
        let changed = choose |index: int| #![trigger before.spec_workers()[index]] {
            &&& 0 <= index < before.spec_workers().len()
            &&& before.spec_workers()[index].spec_descriptor().spec_id() == id
            &&& crate::WorkerRecord::reservation_owner_equivalent(
                &before.spec_workers()[index],
                &after.spec_workers()[index],
            )
            &&& after.spec_workers()[index].spec_descriptor()
                == before.spec_workers()[index].spec_descriptor()
            &&& after.spec_workers()[index].spec_phase() == phase
            &&& forall |other: int| #![auto]
                0 <= other < before.spec_workers().len() && other != index ==>
                    after.spec_workers()[other] == before.spec_workers()[other]
        };
        crate::WorkerRecord::reservation_owner_fields(
            &before.spec_workers()[changed],
            &after.spec_workers()[changed],
        );
        assert forall |index: int| 0 <= index < before.spec_workers().len() implies
            after.spec_workers()[index].spec_descriptor().spec_id()
                == before.spec_workers()[index].spec_descriptor().spec_id() by {
            if index != changed {
                assert(after.spec_workers()[index] == before.spec_workers()[index]);
            }
        }
        assert forall |left: int, right: int|
            0 <= left < right < after.spec_workers().len() implies
                after.spec_workers()[left].spec_descriptor().spec_id().spec_precedes(
                    &after.spec_workers()[right].spec_descriptor().spec_id(),
                ) by {
            assert(before.spec_workers()[left].spec_descriptor().spec_id().spec_precedes(
                &before.spec_workers()[right].spec_descriptor().spec_id(),
            ));
        }
    } else {
        assert(after.spec_workers() == before.spec_workers());
    }
}

fn set_worker_phase_at(
    state: &mut SchedulerState,
    index: usize,
    _id: WorkerId,
    phase: WorkerPhase,
)
    requires
        index < state.spec_workers().len(),
        state.spec_workers()[index as int].spec_descriptor().spec_id() == _id,
    ensures
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        worker_phase_update_matches(
            old(state).spec_workers(),
            final(state).spec_workers(),
            _id,
            phase,
            true,
        ),
        final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_sequence() == old(state).spec_sequence(),
        final(state).spec_last_event_id() == old(state).spec_last_event_id(),
        final(state).spec_state_digest() == old(state).spec_state_digest(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        final(state).spec_enqueue_ordinal() == old(state).spec_enqueue_ordinal(),
        final(state).spec_dispatch_ordinal() == old(state).spec_dispatch_ordinal(),
        final(state).spec_used_commands() == old(state).spec_used_commands(),
        final(state).spec_terminal() == old(state).spec_terminal(),
        worker_phase_state_matches(old(state), final(state), _id, phase, true),
        old(state).spec_collections_ordered() ==> final(state).spec_collections_ordered(),
{
    let ghost before = state.spec_workers();
    let ghost had_invariant = state.spec_reservation_invariant();
    let ghost was_ready = state.spec_reservation_reducer_ready();
    let ghost was_ordered = state.spec_collections_ordered();
    state.workers[index].set_phase(phase);
    proof {
        assert(before.len() == state.spec_workers().len());
        assert(crate::WorkerRecord::reservation_owner_equivalent(
            &before[index as int],
            &state.spec_workers()[index as int],
        ));
        assert(state.spec_workers()[index as int].spec_phase() == phase);
        assert forall |other: int| #![auto]
            0 <= other < before.len() && other != index
                implies before[other] == state.spec_workers()[other] by {
        }
        if had_invariant {
            crate::verified::actual_reservation_worker_update_preserves(
                state.spec_binding(),
                before,
                state.spec_workers(),
                state.spec_work(),
                state.spec_reservations(),
                index as int,
            );
        }
        if was_ready {
            reveal(SchedulerState::spec_reservation_reducer_ready);
        }
        reveal(worker_phase_update_matches);
        assert(exists |found_at: int| #![trigger before[found_at]] {
            &&& 0 <= found_at < before.len()
            &&& before[found_at].spec_descriptor().spec_id() == _id
            &&& crate::WorkerRecord::reservation_owner_equivalent(
                &before[found_at],
                &state.spec_workers()[found_at],
            )
            &&& state.spec_workers()[found_at].spec_descriptor()
                == before[found_at].spec_descriptor()
            &&& state.spec_workers()[found_at].spec_phase() == phase
            &&& forall |other: int| #![auto]
                0 <= other < before.len() && other != found_at ==>
                    state.spec_workers()[other] == before[other]
        }) by {
            assert(0 <= (index as int) && (index as int) < before.len());
        }
        reveal(worker_phase_state_matches);
        reveal(worker_update_preserves_other_state);
        assert(worker_phase_state_matches(old(state), state, _id, phase, true));
        if was_ordered {
            worker_phase_state_preserves_collections_order(
                old(state), state, _id, phase, true,
            );
        }
    };
}

fn scan_and_set_worker_phase(
    state: &mut SchedulerState,
    id: WorkerId,
    phase: WorkerPhase,
) -> (found: bool)
    ensures
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        worker_phase_state_matches(old(state), final(state), id, phase, found),
        old(state).spec_collections_ordered() ==> final(state).spec_collections_ordered(),
{
    let ghost initial = *state;
    let mut index = 0;
    while index < state.workers.len()
        invariant
            index <= state.workers@.len(),
            initial == *old(state),
            *state == initial,
            forall |prior: int| #![auto]
                0 <= prior < index ==>
                    initial.spec_workers()[prior].spec_descriptor().spec_id() != id,
        decreases state.workers@.len() - index,
    {
        if state.workers[index].descriptor().id().same(&id) {
            proof {
                assert(initial.spec_workers()[index as int].spec_descriptor().spec_id() == id);
            }
            set_worker_phase_at(state, index, id, phase);
            proof {
                assert(initial == *old(state));
                assert(old(state).spec_reservation_invariant()
                    ==> state.spec_reservation_invariant());
                assert(old(state).spec_reservation_reducer_ready()
                    ==> state.spec_reservation_reducer_ready());
                assert(worker_phase_state_matches(old(state), state, id, phase, true));
                assert(old(state).spec_collections_ordered()
                    ==> state.spec_collections_ordered());
            }
            return true;
        }
        proof {
            assert(initial.spec_workers()[index as int].spec_descriptor().spec_id() != id);
        }
        index += 1;
    }
    proof {
        assert forall |at: int| #![trigger initial.spec_workers()[at]]
            0 <= at < initial.spec_workers().len() implies
                initial.spec_workers()[at].spec_descriptor().spec_id() != id by {
            assert(at < index);
        }
        reveal(worker_phase_state_matches);
        reveal(worker_phase_update_matches);
        reveal(worker_update_preserves_other_state);
    }
    false
}

pub fn set_worker_phase(
    state: &mut SchedulerState,
    id: WorkerId,
    phase: WorkerPhase,
) -> (found: bool)
    ensures
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        worker_phase_update_matches(
            old(state).spec_workers(),
            final(state).spec_workers(),
            id,
            phase,
            found,
        ),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        worker_phase_state_matches(old(state), final(state), id, phase, found),
        old(state).spec_collections_ordered() ==> final(state).spec_collections_ordered(),
{
    if let Some(index) = state.worker_index(id) {
        proof {
            assert(state.spec_workers()[index as int].spec_descriptor().spec_id() == id);
        }
        set_worker_phase_at(state, index, id, phase);
        true
    } else {
        scan_and_set_worker_phase(state, id, phase)
    }
}

} // verus!
