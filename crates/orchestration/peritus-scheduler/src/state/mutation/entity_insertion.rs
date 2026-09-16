//! Production worker and work insertion with actual-state preservation contracts.

use vstd::prelude::*;

use crate::{SchedulerState, WorkRecord, WorkerRecord};

mod slots;
use slots::{work_slot, worker_slot};

verus! {

/// Inserts one worker at the same canonical binary-search position used by reducer admission.
pub fn insert_worker(state: &mut SchedulerState, value: WorkerRecord)
    ensures
        exists |at: int| #![auto]
            0 <= at <= old(state).spec_workers().len()
                && final(state).spec_workers() == old(state).spec_workers().insert(at, value),
        final(state).spec_binding().spec_limits()
            == old(state).spec_binding().spec_limits(),
        final(state).spec_binding().spec_capacity().spec_entries()
            == old(state).spec_binding().spec_capacity().spec_entries(),
        final(state).spec_work() == old(state).spec_work(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        old(state).spec_workers_ordered()
                && crate::verified::worker_identity_absent(
                    old(state).spec_workers(), value.spec_descriptor().spec_id(),
                )
            ==> final(state).spec_workers_ordered(),
        old(state).spec_reservation_invariant()
                && crate::verified::worker_identity_absent(
                    old(state).spec_workers(),
                    value.spec_descriptor().spec_id(),
                )
            ==> final(state).spec_reservation_invariant(),
        old(state).spec_reservation_reducer_ready()
                && crate::verified::worker_identity_absent(
                    old(state).spec_workers(),
                    value.spec_descriptor().spec_id(),
                )
            ==> final(state).spec_reservation_reducer_ready(),
{
    let ghost had_invariant = state.spec_reservation_invariant();
    let ghost was_absent = crate::verified::worker_identity_absent(
        state.spec_workers(),
        value.spec_descriptor().spec_id(),
    );
    let ghost was_ready = state.spec_reservation_reducer_ready();
    let ghost before = state.spec_workers();
    let at = worker_slot(&state.workers, value.descriptor().id());
    proof {
        assert(0 <= at as int <= state.spec_workers().len());
        if state.spec_workers_ordered() && was_absent {
            SchedulerState::worker_insertion_ordered(before, value, at as int);
        }
        if had_invariant && was_absent {
            crate::verified::actual_worker_insertion_preserves(
                state.spec_binding(),
                state.spec_workers(),
                state.spec_work(),
                state.spec_reservations(),
                value,
                at as int,
            );
        }
        if was_ready && was_absent {
            reveal(SchedulerState::spec_reservation_reducer_ready);
        }
    };
    state.workers.insert(at, value);
    proof {
        assert(state.spec_workers() == before.insert(at as int, value));
        if had_invariant && was_absent {
            assert(state.spec_reservation_invariant());
        }
        if was_ready && was_absent {
            assert(state.spec_work() == old(state).spec_work());
            assert(state.spec_reservations() == old(state).spec_reservations());
            assert(state.spec_reservation_reducer_ready());
        }
    };
}

/// Inserts one work record at the same canonical binary-search position used by admission.
pub fn insert_work(state: &mut SchedulerState, value: WorkRecord)
    ensures
        super::work_update_preserves_other_state(old(state), final(state)),
        exists |at: int| #![auto]
            0 <= at <= old(state).spec_work().len()
                && final(state).spec_work() == old(state).spec_work().insert(at, value),
        final(state).spec_binding().spec_limits()
            == old(state).spec_binding().spec_limits(),
        final(state).spec_binding().spec_capacity().spec_entries()
            == old(state).spec_binding().spec_capacity().spec_entries(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        old(state).spec_work_ordered()
                && crate::verified::work_identity_absent(
                    old(state).spec_work(), value.spec_definition().spec_id(),
                )
            ==> final(state).spec_work_ordered(),
        old(state).spec_reservation_invariant()
                && crate::verified::work_identity_absent(
                    old(state).spec_work(),
                    value.spec_definition().spec_id(),
                )
            ==> final(state).spec_reservation_invariant(),
        old(state).spec_reservation_reducer_ready()
                && crate::verified::work_identity_absent(
                    old(state).spec_work(),
                    value.spec_definition().spec_id(),
                )
                && !crate::verified::work_phase_retains_reservation(value.spec_phase())
            ==> final(state).spec_reservation_reducer_ready(),
{
    let ghost had_invariant = state.spec_reservation_invariant();
    let ghost was_absent = crate::verified::work_identity_absent(
        state.spec_work(),
        value.spec_definition().spec_id(),
    );
    let ghost was_ready = state.spec_reservation_reducer_ready();
    let ghost before = state.spec_work();
    let at = work_slot(&state.work, value.spec().id());
    proof {
        assert(0 <= at as int <= state.spec_work().len());
        if state.spec_work_ordered() && was_absent {
            SchedulerState::work_insertion_ordered(before, value, at as int);
        }
        if had_invariant && was_absent {
            crate::verified::actual_work_insertion_preserves(
                state.spec_binding(),
                state.spec_workers(),
                state.spec_work(),
                state.spec_reservations(),
                value,
                at as int,
            );
        }
        if was_ready && was_absent
            && !crate::verified::work_phase_retains_reservation(value.spec_phase())
        {
            crate::verified::inactive_work_insertion_preserves_phase(
                state.spec_work(),
                state.spec_reservations(),
                value,
                at as int,
            );
        }
    }
    state.work.insert(at, value);
    proof {
        reveal(super::work_update_preserves_other_state);
        assert(state.spec_work() == before.insert(at as int, value));
        if had_invariant && was_absent {
            assert(state.spec_reservation_invariant());
        }
        if was_ready && was_absent
            && !crate::verified::work_phase_retains_reservation(value.spec_phase())
        {
            assert(state.spec_reservation_reducer_ready());
        }
    };
}

} // verus!
