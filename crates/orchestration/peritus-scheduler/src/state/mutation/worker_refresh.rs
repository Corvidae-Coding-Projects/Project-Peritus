//! Exact worker availability refresh after reservation mutations.

mod step;

#[cfg(verus_only)]
use crate::state::mutation;
#[cfg(verus_only)]
use crate::{SchedulerReservation, WorkerPhase};
use crate::{SchedulerState, WorkerId, WorkerRecord};
use step::refresh_one_worker;
#[cfg(verus_only)]
use step::worker_refresh_step_matches;
use vstd::prelude::*;

verus! {

/// Exact phase selected from immutable worker policy and retained reservation ownership.
pub open spec fn refreshed_worker_phase(
    worker: WorkerRecord,
    reservations: Seq<SchedulerReservation>,
) -> WorkerPhase {
    match worker.spec_phase() {
        WorkerPhase::Draining | WorkerPhase::Lost | WorkerPhase::Removed => worker.spec_phase(),
        WorkerPhase::Available | WorkerPhase::Busy => {
            if crate::verified::worker_count(
                reservations, worker.spec_descriptor().spec_id(),
            ) >= worker.spec_descriptor().spec_concurrency() as int {
                WorkerPhase::Busy
            } else {
                WorkerPhase::Available
            }
        },
    }
}

/// Exact descriptor layout retained by the refresh loop.
pub open spec fn worker_layout_matches(
    before: Seq<WorkerRecord>,
    after: Seq<WorkerRecord>,
) -> bool {
    &&& before.len() == after.len()
    &&& forall |index: int| #![trigger before[index]] 0 <= index < before.len() ==>
        after[index].spec_descriptor() == before[index].spec_descriptor()
}

/// Complete final worker sequence produced from the original reservation snapshot.
pub open spec fn worker_refresh_matches(
    before: &SchedulerState,
    after: &SchedulerState,
) -> bool {
    &&& worker_layout_matches(before.spec_workers(), after.spec_workers())
    &&& (forall |index: int| #![trigger after.spec_workers()[index]]
        0 <= index < after.spec_workers().len() ==>
            after.spec_workers()[index].spec_phase()
                == refreshed_worker_phase(
                    before.spec_workers()[index], before.spec_reservations(),
                ))
    &&& mutation::worker_update_preserves_other_state(before, after)
}

spec fn worker_ids_match(workers: Seq<WorkerRecord>, ids: Seq<WorkerId>) -> bool {
    &&& ids.len() == workers.len()
    &&& forall |index: int| #![trigger workers[index]] 0 <= index < workers.len() ==>
        ids[index] == workers[index].spec_descriptor().spec_id()
}

fn retained_worker_ids(workers: &[WorkerRecord]) -> (ids: Vec<WorkerId>)
    ensures worker_ids_match(workers@, ids@),
{
    let mut ids = Vec::new();
    let mut index = 0;
    while index < workers.len()
        invariant
            index <= workers@.len(),
            ids@.len() == index,
            forall |prior: int| #![trigger workers@[prior]] 0 <= prior < index ==>
                ids@[prior] == workers@[prior].spec_descriptor().spec_id(),
        decreases workers@.len() - index,
    {
        ids.push(workers[index].descriptor().id());
        index += 1;
    }
    proof {
        reveal(worker_ids_match);
        assert(index == workers@.len());
    }
    ids
}

proof fn worker_layout_transitive(
    initial: Seq<WorkerRecord>,
    middle: Seq<WorkerRecord>,
    after: Seq<WorkerRecord>,
)
    requires
        worker_layout_matches(initial, middle),
        worker_layout_matches(middle, after),
    ensures worker_layout_matches(initial, after),
{
    reveal(worker_layout_matches);
    assert forall |index: int| #![trigger initial[index]] 0 <= index < initial.len()
        implies after[index].spec_descriptor() == initial[index].spec_descriptor() by {
    }
}

/// Refreshes every mutable worker against the exact retained reservation count.
pub(super) fn refresh_worker_phases(state: &mut SchedulerState)
    ensures
        old(state).spec_reservation_reducer_ready()
            ==> final(state).spec_reservation_reducer_ready(),
        old(state).spec_collections_ordered() ==> final(state).spec_collections_ordered(),
        crate::state::queue::queue_bound(old(state))
            ==> crate::state::queue::queue_bound(final(state)),
        mutation::worker_update_preserves_other_state(old(state), final(state)),
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
            ==> worker_refresh_matches(old(state), final(state)),
{
    let ghost initial = *state;
    let ghost was_ready = state.spec_reservation_reducer_ready();
    let ghost was_ordered = state.spec_collections_ordered();
    let ghost had_queue_bound = crate::state::queue::queue_bound(state);
    let ids = retained_worker_ids(state.workers());
    let mut index = 0;
    proof {
        reveal(mutation::worker_update_preserves_other_state);
        reveal(worker_layout_matches);
    }
    while index < ids.len()
        invariant
            index <= ids@.len(),
            initial == *old(state),
            worker_ids_match(initial.spec_workers(), ids@),
            was_ready == old(state).spec_reservation_reducer_ready(),
            was_ordered == old(state).spec_collections_ordered(),
            had_queue_bound == crate::state::queue::queue_bound(old(state)),
            was_ready ==> state.spec_reservation_reducer_ready(),
            was_ordered ==> state.spec_collections_ordered(),
            had_queue_bound ==> crate::state::queue::queue_bound(state),
            mutation::worker_update_preserves_other_state(old(state), state),
            worker_layout_matches(initial.spec_workers(), state.spec_workers()),
            was_ready && was_ordered ==> forall |processed: int|
                #![trigger state.spec_workers()[processed]]
                0 <= processed < index ==>
                    state.spec_workers()[processed].spec_phase()
                        == refreshed_worker_phase(
                            initial.spec_workers()[processed], initial.spec_reservations(),
                        ),
            was_ready && was_ordered ==> forall |pending: int|
                #![trigger state.spec_workers()[pending]]
                index <= pending < state.spec_workers().len() ==>
                    state.spec_workers()[pending] == initial.spec_workers()[pending],
        decreases ids@.len() - index,
    {
        let id = ids[index];
        let ghost before = *state;
        refresh_one_worker(state, id, index);
        proof {
            reveal(worker_refresh_step_matches);
            if was_ready && was_ordered {
                reveal(worker_ids_match);
                reveal(worker_layout_matches);
                reveal(mutation::worker_update_preserves_other_state);
                assert(0 <= (index as int) < before.spec_workers().len());
                assert(before.spec_workers()[index as int].spec_descriptor().spec_id() == id);
                assert(before.spec_workers()[index as int] == initial.spec_workers()[index as int]);
                assert(before.spec_reservations() == initial.spec_reservations());
                assert forall |processed: int| #![trigger state.spec_workers()[processed]]
                    0 <= processed < index + 1 implies
                        state.spec_workers()[processed].spec_phase()
                            == refreshed_worker_phase(
                                initial.spec_workers()[processed], initial.spec_reservations(),
                            ) by {
                    if processed < index {
                        assert(state.spec_workers()[processed] == before.spec_workers()[processed]);
                    } else {
                        assert(processed == index);
                    }
                }
                assert forall |pending: int| #![trigger state.spec_workers()[pending]]
                    index + 1 <= pending < state.spec_workers().len()
                    implies state.spec_workers()[pending] == initial.spec_workers()[pending] by {
                    assert(state.spec_workers()[pending] == before.spec_workers()[pending]);
                }
            }
            worker_layout_transitive(
                initial.spec_workers(), before.spec_workers(), state.spec_workers(),
            );
            reveal(mutation::worker_update_preserves_other_state);
        };
        index += 1;
    }
    proof {
        if was_ready && was_ordered {
            assert forall |at: int| #![trigger state.spec_workers()[at]]
                0 <= at < state.spec_workers().len() implies
                    state.spec_workers()[at].spec_phase()
                        == refreshed_worker_phase(
                            initial.spec_workers()[at], initial.spec_reservations(),
                        ) by {
                reveal(worker_ids_match);
                reveal(worker_layout_matches);
                assert(at < index);
            }
            reveal(worker_refresh_matches);
        }
    };
}

} // verus!
