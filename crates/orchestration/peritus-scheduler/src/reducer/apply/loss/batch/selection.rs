//! Selection facts connecting the canonical pre-state projection to batch lookups.

use peritus_types::Sha256Digest;
use vstd::prelude::*;

use crate::{DispatchId, SchedulerReservation, SchedulerState, WorkerId};

use super::super::release_one::dispatch_exists;
use super::super::selected_dispatches;
use super::plan_dispatches;

verus! {

proof fn selected_dispatch_origin(
    reservations: Seq<SchedulerReservation>,
    worker_id: WorkerId,
    selected_index: int,
)
    requires
        0 <= selected_index < selected_dispatches(reservations, worker_id).len(),
    ensures exists |source: int| #![trigger reservations[source]] {
        &&& 0 <= source < reservations.len()
        &&& reservations[source].spec_worker_id() == worker_id
        &&& reservations[source].spec_dispatch_id()
            == selected_dispatches(reservations, worker_id)[selected_index]
    },
    decreases reservations.len(),
{
    reveal(selected_dispatches);
    reveal(Seq::filter);
    assert(reservations.len() > 0);
    let prefix = reservations.drop_last();
    let last = reservations.last();
    let predicate = |reservation: SchedulerReservation| {
        reservation.spec_worker_id() == worker_id
    };
    prefix.lemma_filter_push(last, predicate);
    assert(prefix.push(last) =~= reservations);
    let prior = selected_dispatches(prefix, worker_id);
    let selected = selected_dispatches(reservations, worker_id);
    if last.spec_worker_id() == worker_id {
        assert(selected =~= prior.push(last.spec_dispatch_id()));
        if selected_index < prior.len() {
            selected_dispatch_origin(prefix, worker_id, selected_index);
            let source = choose |source: int| #![trigger prefix[source]] {
                &&& 0 <= source < prefix.len()
                &&& prefix[source].spec_worker_id() == worker_id
                &&& prefix[source].spec_dispatch_id() == prior[selected_index]
            };
            assert(prefix[source] == reservations[source]);
            assert(exists |at: int| #![trigger reservations[at]] {
                &&& 0 <= at < reservations.len()
                &&& reservations[at].spec_worker_id() == worker_id
                &&& reservations[at].spec_dispatch_id() == selected[selected_index]
            }) by {
                assert(source < reservations.len());
            }
        } else {
            assert(selected_index == prior.len());
            let source = reservations.len() - 1;
            assert(reservations[source] == last);
            assert(exists |at: int| #![trigger reservations[at]] {
                &&& 0 <= at < reservations.len()
                &&& reservations[at].spec_worker_id() == worker_id
                &&& reservations[at].spec_dispatch_id() == selected[selected_index]
            }) by {
                assert(source == reservations.len() - 1);
            }
        }
    } else {
        assert(selected =~= prior);
        selected_dispatch_origin(prefix, worker_id, selected_index);
        let source = choose |source: int| #![trigger prefix[source]] {
            &&& 0 <= source < prefix.len()
            &&& prefix[source].spec_worker_id() == worker_id
            &&& prefix[source].spec_dispatch_id() == prior[selected_index]
        };
        assert(prefix[source] == reservations[source]);
        assert(exists |at: int| #![trigger reservations[at]] {
            &&& 0 <= at < reservations.len()
            &&& reservations[at].spec_worker_id() == worker_id
            &&& reservations[at].spec_dispatch_id() == selected[selected_index]
        }) by {
            assert(source < reservations.len());
        }
    }
}

proof fn selected_dispatches_are_ordered(
    reservations: Seq<SchedulerReservation>,
    worker_id: WorkerId,
)
    requires SchedulerState::reservation_records_ordered(reservations),
    ensures forall |left: int, right: int|
        0 <= left < right < selected_dispatches(reservations, worker_id).len() ==>
            selected_dispatches(reservations, worker_id)[left].spec_precedes(
                &selected_dispatches(reservations, worker_id)[right],
            ),
    decreases reservations.len(),
{
    reveal(selected_dispatches);
    reveal(Seq::filter);
    if reservations.len() == 0 {
        return;
    }
    let prefix = reservations.drop_last();
    let last = reservations.last();
    let predicate = |reservation: SchedulerReservation| {
        reservation.spec_worker_id() == worker_id
    };
    prefix.lemma_filter_push(last, predicate);
    assert(prefix.push(last) =~= reservations);
    assert(SchedulerState::reservation_records_ordered(prefix)) by {
        reveal(SchedulerState::reservation_records_ordered);
        assert forall |left: int, right: int| 0 <= left < right < prefix.len() implies
            prefix[left].spec_dispatch_id().spec_precedes(
                &prefix[right].spec_dispatch_id(),
            ) by {
            assert(prefix[left] == reservations[left]);
            assert(prefix[right] == reservations[right]);
        }
    }
    selected_dispatches_are_ordered(prefix, worker_id);
    let prior = selected_dispatches(prefix, worker_id);
    let selected = selected_dispatches(reservations, worker_id);
    if last.spec_worker_id() == worker_id {
        assert(selected =~= prior.push(last.spec_dispatch_id()));
        assert forall |left: int, right: int| 0 <= left < right < selected.len() implies
            selected[left].spec_precedes(&selected[right]) by {
            if right < prior.len() {
                assert(prior[left].spec_precedes(&prior[right]));
            } else {
                assert(right == prior.len());
                selected_dispatch_origin(prefix, worker_id, left);
                let source = choose |source: int| #![trigger prefix[source]] {
                    &&& 0 <= source < prefix.len()
                    &&& prefix[source].spec_worker_id() == worker_id
                    &&& prefix[source].spec_dispatch_id() == prior[left]
                };
                assert(prefix[source] == reservations[source]);
                assert(reservations[source].spec_dispatch_id().spec_precedes(
                    &reservations[reservations.len() - 1].spec_dispatch_id(),
                ));
            }
        }
    } else {
        assert(selected =~= prior);
    }
}

/// Canonical reservation ordering gives the selected dispatch projection unique identities.
pub(super) proof fn selected_dispatches_are_unique(
    reservations: Seq<SchedulerReservation>,
    worker_id: WorkerId,
)
    requires SchedulerState::reservation_records_ordered(reservations),
    ensures selected_dispatches(reservations, worker_id).no_duplicates(),
{
    selected_dispatches_are_ordered(reservations, worker_id);
    let selected = selected_dispatches(reservations, worker_id);
    assert forall |left: int, right: int|
        0 <= left < right < selected.len() implies selected[left] != selected[right] by {
        if selected[left] == selected[right] {
            DispatchId::order_irreflexive(&selected[left]);
        }
    }
}

/// The exact selected plan starts with every planned dispatch retained.
pub(super) proof fn plan_dispatches_are_retained(
    state: &SchedulerState,
    worker_id: WorkerId,
    plan: Seq<(DispatchId, Sha256Digest)>,
)
    requires
        plan_dispatches(plan) == selected_dispatches(state.spec_reservations(), worker_id),
    ensures forall |index: int| #![trigger plan[index]] 0 <= index < plan.len() ==>
        dispatch_exists(state, plan[index].0),
{
    reveal(plan_dispatches);
    assert(plan.len() == selected_dispatches(state.spec_reservations(), worker_id).len());
    assert forall |index: int| #![trigger plan[index]] 0 <= index < plan.len() implies
        dispatch_exists(state, plan[index].0) by {
        assert(0 <= index
            < selected_dispatches(state.spec_reservations(), worker_id).len());
        selected_dispatch_origin(state.spec_reservations(), worker_id, index);
        let source = choose |source: int| #![trigger state.spec_reservations()[source]] {
            &&& 0 <= source < state.spec_reservations().len()
            &&& state.spec_reservations()[source].spec_worker_id() == worker_id
            &&& state.spec_reservations()[source].spec_dispatch_id()
                == selected_dispatches(state.spec_reservations(), worker_id)[index]
        };
        assert(plan_dispatches(plan)[index] == plan[index].0);
        assert(plan[index].0
            == selected_dispatches(state.spec_reservations(), worker_id)[index]);
        reveal(dispatch_exists);
        assert(exists |at: int| #![trigger state.spec_reservations()[at]]
            0 <= at < state.spec_reservations().len()
                && state.spec_reservations()[at].spec_dispatch_id() == plan[index].0) by {
            assert(source < state.spec_reservations().len());
            assert(state.spec_reservations()[source].spec_dispatch_id() == plan[index].0);
        }
    }
}

} // verus!
