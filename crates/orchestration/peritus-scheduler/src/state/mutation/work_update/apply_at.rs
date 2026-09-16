//! Exact lifecycle mutation at an already-validated retained work index.

use super::WorkUpdate;
#[cfg(verus_only)]
use super::{
    exact_work_update_matches, work_record_update_matches, work_update_matches,
    work_update_preserves_other_state,
};
#[cfg(verus_only)]
use crate::WorkRecord;
use crate::{SchedulerState, WorkId};
use vstd::prelude::*;

verus! {

pub(super) fn update_work_at(
    state: &mut SchedulerState,
    index: usize,
    _id: WorkId,
    update: WorkUpdate,
)
    requires
        index < state.spec_work().len(),
        state.spec_work()[index as int].spec_definition().spec_id() == _id,
    ensures
        old(state).spec_reservation_invariant()
            ==> final(state).spec_reservation_invariant(),
        work_update_preserves_other_state(old(state), final(state)),
        final(state).spec_phase() == old(state).spec_phase(),
        final(state).spec_binding() == old(state).spec_binding(),
        final(state).spec_workers() == old(state).spec_workers(),
        final(state).spec_reservations() == old(state).spec_reservations(),
        final(state).spec_used_dispatches() == old(state).spec_used_dispatches(),
        work_update_matches(
            old(state).spec_work(),
            final(state).spec_work(),
            _id,
            update.spec_phase(),
            true,
        ),
        exact_work_update_matches(
            old(state).spec_work(),
            final(state).spec_work(),
            _id,
            update,
            true,
        ),
        old(state).spec_reservation_reducer_ready()
                && crate::verified::work_phase_update_admissible(
                    old(state).spec_work(),
                    old(state).spec_reservations(),
                    _id,
                    update.spec_phase(),
                )
            ==> final(state).spec_reservation_reducer_ready(),
{
    let ghost had_invariant = state.spec_reservation_invariant();
    let ghost before = state.spec_work();
    let ghost reservations = state.spec_reservations();
    let ghost used_dispatches = state.spec_used_dispatches();
    let ghost was_ready = state.spec_reservation_reducer_ready();
    let ghost requested_update = update;
    let ghost target_phase = update.spec_phase();
    let ghost was_admissible = crate::verified::work_phase_update_admissible(
        before,
        reservations,
        _id,
        target_phase,
    );
    match update {
        WorkUpdate::Phase(phase) => state.work[index].set_phase(phase),
        WorkUpdate::QueueRetry => state.work[index].queue_retry(),
        WorkUpdate::RetryPending(cause) => state.work[index].set_retry_pending(cause),
        WorkUpdate::Terminal(terminal) => state.work[index].terminalize(terminal),
    }
    proof {
        assert(before.len() == state.spec_work().len());
        assert(WorkRecord::reservation_binding_equivalent(
            &before[index as int],
            &state.spec_work()[index as int],
        ));
        assert(WorkRecord::lifecycle_update_stable(
            &before[index as int],
            &state.spec_work()[index as int],
        ));
        assert(state.spec_work()[index as int].spec_phase() == target_phase);
        assert(requested_update.record_matches(
            before[index as int],
            state.spec_work()[index as int],
            _id,
        ));
        assert forall |other: int| #![auto]
            0 <= other < before.len() && other != index
                implies before[other] == state.spec_work()[other] by {
        }
        if had_invariant {
            crate::verified::actual_reservation_work_update_preserves(
                state.spec_binding(),
                state.spec_workers(),
                before,
                state.spec_work(),
                state.spec_reservations(),
                index as int,
            );
        }
        reveal(work_record_update_matches);
        reveal(work_update_matches);
        reveal(exact_work_update_matches);
        assert(exists |found_at: int| #![trigger before[found_at]] {
            &&& 0 <= found_at < before.len()
            &&& work_record_update_matches(
                before[found_at],
                state.spec_work()[found_at],
                _id,
                target_phase,
            )
            &&& forall |other: int| #![auto]
                0 <= other < before.len() && other != found_at ==>
                    state.spec_work()[other] == before[other]
        }) by {
            assert(0 <= (index as int) && (index as int) < before.len());
        }
        assert(exists |found_at: int| #![trigger before[found_at]] {
            &&& 0 <= found_at < before.len()
            &&& requested_update.record_matches(
                before[found_at],
                state.spec_work()[found_at],
                _id,
            )
            &&& forall |other: int| #![auto]
                0 <= other < before.len() && other != found_at ==>
                    state.spec_work()[other] == before[other]
        }) by {
            assert(0 <= (index as int) && (index as int) < before.len());
        }
        if was_ready && was_admissible {
            reveal(SchedulerState::spec_reservation_reducer_ready);
            reveal(crate::verified::reservation_invariant_parts);
            crate::verified::admissible_work_update_preserves_relations(
                before,
                state.spec_work(),
                reservations,
                used_dispatches,
                _id,
                target_phase,
                index as int,
            );
        }
    };
}

} // verus!
