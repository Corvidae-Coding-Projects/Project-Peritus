//! Exact effect of refreshing one worker from the retained identity snapshot.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::WorkerRecord;
use crate::state::mutation;
use crate::{SchedulerState, WorkerId, WorkerPhase};

#[cfg(verus_only)]
use super::{refreshed_worker_phase, worker_layout_matches};

verus! {

/// Complete state and target effect of one worker refresh attempt.
pub open spec fn worker_refresh_step_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    id: WorkerId,
    index: int,
) -> bool {
    &&& worker_layout_matches(before.spec_workers(), after.spec_workers())
    &&& mutation::worker_update_preserves_other_state(before, after)
    &&& (before.spec_reservation_reducer_ready()
            && before.spec_collections_ordered()
            && 0 <= index < before.spec_workers().len()
            && before.spec_workers()[index].spec_descriptor().spec_id() == id ==>
        after.spec_workers()[index].spec_phase()
            == refreshed_worker_phase(before.spec_workers()[index], before.spec_reservations())
        && forall |other: int| #![auto]
            0 <= other < before.spec_workers().len() && other != index ==>
                after.spec_workers()[other] == before.spec_workers()[other])
}

proof fn phase_update_preserves_layout(
    before: Seq<WorkerRecord>,
    after: Seq<WorkerRecord>,
    id: WorkerId,
    phase: WorkerPhase,
    found: bool,
)
    requires mutation::worker_phase_update_matches(before, after, id, phase, found),
    ensures worker_layout_matches(before, after),
{
    reveal(mutation::worker_phase_update_matches);
    reveal(worker_layout_matches);
    if found {
        let changed = choose |index: int| #![trigger before[index]] {
            &&& 0 <= index < before.len()
            &&& before[index].spec_descriptor().spec_id() == id
            &&& after[index].spec_descriptor() == before[index].spec_descriptor()
            &&& after[index].spec_phase() == phase
            &&& forall |other: int| #![auto]
                0 <= other < before.len() && other != index ==> after[other] == before[other]
        };
        assert forall |index: int| #![trigger before[index]] 0 <= index < before.len()
            implies after[index].spec_descriptor() == before[index].spec_descriptor() by {
            if index != changed {
                assert(after[index] == before[index]);
            }
        }
    }
}

proof fn exact_target_update(
    before: Seq<WorkerRecord>,
    after: Seq<WorkerRecord>,
    id: WorkerId,
    phase: WorkerPhase,
    index: int,
)
    requires
        mutation::worker_phase_update_matches(before, after, id, phase, true),
        0 <= index < before.len(),
        before[index].spec_descriptor().spec_id() == id,
        crate::verified::worker_identities_unique(before),
    ensures
        after[index].spec_phase() == phase,
        forall |other: int| #![auto]
            0 <= other < before.len() && other != index ==> after[other] == before[other],
{
    reveal(mutation::worker_phase_update_matches);
    let changed = choose |changed: int| #![trigger before[changed]] {
        &&& 0 <= changed < before.len()
        &&& before[changed].spec_descriptor().spec_id() == id
        &&& after[changed].spec_descriptor() == before[changed].spec_descriptor()
        &&& after[changed].spec_phase() == phase
        &&& forall |other: int| #![auto]
            0 <= other < before.len() && other != changed ==> after[other] == before[other]
    };
    reveal(crate::verified::worker_identities_unique);
    assert(changed == index);
}

proof fn selected_worker_is_exact(
    state: &SchedulerState,
    id: WorkerId,
    index: int,
    record: WorkerRecord,
)
    requires
        state.spec_reservation_reducer_ready(),
        0 <= index < state.spec_workers().len(),
        state.spec_workers()[index].spec_descriptor().spec_id() == id,
        record.spec_descriptor().spec_id() == id,
        exists |found: int| #![trigger state.spec_workers()[found]]
            0 <= found < state.spec_workers().len()
                && state.spec_workers()[found].spec_descriptor().spec_id() == id
                && state.spec_workers()[found] == record,
    ensures
        crate::verified::worker_identities_unique(state.spec_workers()),
        state.spec_workers()[index] == record,
{
    reveal(SchedulerState::spec_reservation_reducer_ready);
    reveal(crate::verified::reservation_invariant_parts);
    let found = choose |found: int| #![trigger state.spec_workers()[found]]
        0 <= found < state.spec_workers().len()
            && state.spec_workers()[found].spec_descriptor().spec_id() == id
            && state.spec_workers()[found] == record;
    reveal(crate::verified::worker_identities_unique);
    assert(found == index);
}

proof fn refresh_frame_preserves_bound(before: &SchedulerState, after: &SchedulerState)
    requires mutation::worker_update_preserves_other_state(before, after),
    ensures crate::state::queue::queue_bound(before) == crate::state::queue::queue_bound(after),
{
    reveal(mutation::worker_update_preserves_other_state);
    reveal(crate::state::queue::queue_bound);
    reveal(crate::state::queue::admission_pressure);
}

/// Refreshes one retained identity with the same lookup and mutation order as production.
pub(super) fn refresh_one_worker(state: &mut SchedulerState, id: WorkerId, _index: usize)
    ensures
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        old(state).spec_collections_ordered() ==> final(state).spec_collections_ordered(),
        crate::state::queue::queue_bound(old(state))
            ==> crate::state::queue::queue_bound(final(state)),
        worker_refresh_step_matches(old(state), final(state), id, _index as int),
{
    let ghost initial = *state;
    let ghost exact = state.spec_reservation_reducer_ready()
        && state.spec_collections_ordered()
        && 0 <= (_index as int) < state.spec_workers().len()
        && state.spec_workers()[_index as int].spec_descriptor().spec_id() == id;
    let Some(worker) = state.worker(id) else {
        proof {
            if exact {
                reveal(SchedulerState::spec_collections_ordered);
                assert(false);
            }
            reveal(worker_layout_matches);
            reveal(mutation::worker_update_preserves_other_state);
            reveal(worker_refresh_step_matches);
        };
        return;
    };
    let phase = worker.phase();
    proof {
        if exact {
            assert(exists |found: int| #![trigger state.spec_workers()[found]]
                0 <= found < state.spec_workers().len()
                    && state.spec_workers()[found].spec_descriptor().spec_id() == id
                    && state.spec_workers()[found] == *worker);
            selected_worker_is_exact(state, id, _index as int, *worker);
        }
    };
    if matches!(phase, WorkerPhase::Draining | WorkerPhase::Lost | WorkerPhase::Removed) {
        proof {
            if exact {
                reveal(refreshed_worker_phase);
            }
            reveal(worker_layout_matches);
            reveal(mutation::worker_update_preserves_other_state);
            reveal(worker_refresh_step_matches);
        };
        return;
    }
    let concurrency = worker.descriptor().concurrency();
    let active = crate::selection::worker_reservation_count(state, id);
    let target = if active >= usize::from(concurrency) {
        WorkerPhase::Busy
    } else {
        WorkerPhase::Available
    };
    proof {
        if exact {
            assert(state.spec_reservation_invariant()) by {
                reveal(SchedulerState::spec_reservation_reducer_ready);
            }
            assert(active as int == crate::verified::worker_count(
                state.spec_reservations(), id,
            ));
            assert(target == refreshed_worker_phase(
                initial.spec_workers()[_index as int], initial.spec_reservations(),
            )) by {
                reveal(refreshed_worker_phase);
            }
        }
    };
    let _found = mutation::set_worker_phase(state, id, target);
    proof {
        if exact {
            assert(crate::verified::worker_identities_unique(initial.spec_workers()));
            assert(_found) by {
                if !_found {
                    reveal(mutation::worker_phase_update_matches);
                    assert(initial.spec_workers()[_index as int]
                        .spec_descriptor().spec_id() != id);
                    assert(false);
                }
            }
            exact_target_update(
                initial.spec_workers(), state.spec_workers(), id, target, _index as int,
            );
        }
        phase_update_preserves_layout(
            initial.spec_workers(), state.spec_workers(), id, target, _found,
        );
        reveal(mutation::worker_update_preserves_other_state);
        refresh_frame_preserves_bound(old(state), state);
        reveal(worker_refresh_step_matches);
    };
}

} // verus!
