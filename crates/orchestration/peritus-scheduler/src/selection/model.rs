//! Exact mathematical model for production scheduler selection.

use vstd::prelude::*;

use crate::{WorkId, WorkerId};

#[cfg(verus_only)]
use crate::{SchedulerState, WorkPhase, WorkerPhase};

verus! {

/// One deterministic feasible work/worker choice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Selection {
    pub(super) work_id: WorkId,
    pub(super) worker_id: WorkerId,
}

impl Selection {
    /// Returns the mathematical selected work identity.
    pub closed spec fn spec_work_id(&self) -> WorkId { self.work_id }
    /// Returns the mathematical selected worker identity.
    pub closed spec fn spec_worker_id(&self) -> WorkerId { self.worker_id }

    pub(super) const fn new(work_id: WorkId, worker_id: WorkerId) -> (result: Self)
        ensures
            result.spec_work_id() == work_id,
            result.spec_worker_id() == worker_id,
    {
        Self { work_id, worker_id }
    }

    /// Returns selected work.
    #[must_use]
    pub const fn work_id(self) -> (result: WorkId)
        ensures result == self.spec_work_id(),
    {
        self.work_id
    }

    /// Returns selected worker.
    #[must_use]
    pub const fn worker_id(self) -> (result: WorkerId)
        ensures result == self.spec_worker_id(),
    {
        self.worker_id
    }
}

/// Exact reservation-capacity and ownership relation established by the production selector.
pub open spec fn selected_pair_feasible_parts(
    binding: &crate::SchedulerBinding,
    workers: Seq<crate::WorkerRecord>,
    work: Seq<crate::WorkRecord>,
    reservations: Seq<crate::SchedulerReservation>,
    work_index: int,
    worker_index: int,
) -> bool {
    &&& 0 <= work_index < work.len()
    &&& 0 <= worker_index < workers.len()
    &&& crate::identity::actor_ids_match(
        workers[worker_index].spec_descriptor().spec_owner(),
        work[work_index].spec_definition().spec_owner(),
    )
    &&& crate::verified::worker_count(
        reservations,
        workers[worker_index].spec_descriptor().spec_id(),
    ) < workers[worker_index].spec_descriptor().spec_concurrency()
    &&& forall |kind: crate::ResourceKind| #![auto]
        crate::verified::reservation_quantity(reservations, kind)
                + crate::verified::vector_quantity(
                    work[work_index].spec_definition()
                        .spec_request().spec_entries(),
                    kind,
                )
            <= binding.spec_capacity().spec_quantity(kind)
    &&& forall |kind: crate::ResourceKind| #![auto]
        crate::verified::worker_quantity(
            reservations,
            workers[worker_index].spec_descriptor().spec_id(),
            kind,
        ) + crate::verified::vector_quantity(
            work[work_index].spec_definition()
                .spec_request().spec_entries(),
            kind,
        ) <= workers[worker_index].spec_descriptor()
            .spec_capacity().spec_quantity(kind)
}

/// Exact selector feasibility projected from one actual scheduler state.
pub open spec fn selected_pair_feasible(
    state: &SchedulerState,
    work_index: int,
    worker_index: int,
) -> bool {
    selected_pair_feasible_parts(
        state.spec_binding(),
        state.spec_workers(),
        state.spec_work(),
        state.spec_reservations(),
        work_index,
        worker_index,
    )
}

/// Complete production admission relation, including lifecycle and execution class checks.
pub open spec fn selected_pair_admitted(
    state: &SchedulerState,
    work_index: int,
    worker_index: int,
) -> bool {
    &&& selected_pair_feasible(state, work_index, worker_index)
    &&& state.spec_work()[work_index].spec_phase() == WorkPhase::Queued
    &&& state.spec_workers()[worker_index].spec_phase() == WorkerPhase::Available
    &&& state.spec_workers()[worker_index].spec_descriptor().spec_classes().contains(
        state.spec_work()[work_index].spec_definition().spec_class(),
    )
}

/// State facts under which the bounded production feasibility checks are exact.
pub open spec fn exact_selection_ready(state: &SchedulerState) -> bool {
    state.spec_reservation_invariant() && state.spec_reservations().len() <= 4_096
}

/// Exact local worker predicate evaluated by the production worker scan.
pub open spec fn worker_candidate(
    state: &SchedulerState,
    work_index: int,
    worker_index: int,
) -> bool {
    &&& 0 <= work_index < state.spec_work().len()
    &&& 0 <= worker_index < state.spec_workers().len()
    &&& state.spec_workers()[worker_index].spec_phase() == WorkerPhase::Available
    &&& crate::identity::actor_ids_match(
        state.spec_workers()[worker_index].spec_descriptor().spec_owner(),
        state.spec_work()[work_index].spec_definition().spec_owner(),
    )
    &&& state.spec_workers()[worker_index].spec_descriptor().spec_classes().contains(
        state.spec_work()[work_index].spec_definition().spec_class(),
    )
    &&& crate::verified::worker_count(
        state.spec_reservations(),
        state.spec_workers()[worker_index].spec_descriptor().spec_id(),
    ) < state.spec_workers()[worker_index].spec_descriptor().spec_concurrency()
    &&& super::capacity::worker_entries_fit_after(
        state,
        state.spec_workers()[worker_index].spec_descriptor().spec_id(),
        state.spec_workers()[worker_index].spec_descriptor().spec_capacity(),
        state.spec_work()[work_index].spec_definition().spec_request().spec_entries(),
    )
}

/// Exact work/worker pair accepted by the production feasibility scans.
pub open spec fn dispatch_candidate(
    state: &SchedulerState,
    work_index: int,
    worker_index: int,
) -> bool {
    &&& 0 <= work_index < state.spec_work().len()
    &&& state.spec_work()[work_index].spec_phase() == WorkPhase::Queued
    &&& super::capacity::global_entries_fit_after(
        state,
        state.spec_work()[work_index].spec_definition().spec_request().spec_entries(),
    )
    &&& worker_candidate(state, work_index, worker_index)
}

/// The worker is the first feasible worker in retained canonical order.
pub open spec fn first_candidate_worker(
    state: &SchedulerState,
    work_index: int,
    worker_index: int,
) -> bool {
    &&& dispatch_candidate(state, work_index, worker_index)
    &&& forall |prior: int| 0 <= prior < worker_index ==>
        !dispatch_candidate(state, work_index, prior)
}

/// At least one retained worker can accept the indexed work item.
pub open spec fn work_has_candidate(state: &SchedulerState, work_index: int) -> bool {
    exists |worker_index: int| #![trigger dispatch_candidate(state, work_index, worker_index)]
        dispatch_candidate(state, work_index, worker_index)
}

/// Exact aged-first, priority, enqueue-ordinal and identity ordering.
pub open spec fn work_precedes(
    state: &SchedulerState,
    left_index: int,
    right_index: int,
) -> bool {
    let left = state.spec_work()[left_index];
    let right = state.spec_work()[right_index];
    let limit = state.spec_binding().spec_limits().spec_bypass_count();
    let left_aged = left.spec_bypasses() >= limit;
    let right_aged = right.spec_bypasses() >= limit;
    if left_aged != right_aged {
        left_aged
    } else if left.spec_definition().spec_priority()
        != right.spec_definition().spec_priority()
    {
        left.spec_definition().spec_priority() > right.spec_definition().spec_priority()
    } else if left.spec_enqueue_ordinal() != right.spec_enqueue_ordinal() {
        left.spec_enqueue_ordinal() < right.spec_enqueue_ordinal()
    } else {
        left.spec_definition().spec_id().spec_precedes(
            &right.spec_definition().spec_id(),
        )
    }
}

/// Exact retained indices selected by the complete production ordering.
pub open spec fn chosen_pair_matches(
    state: &SchedulerState,
    work_index: int,
    worker_index: int,
) -> bool {
    &&& first_candidate_worker(state, work_index, worker_index)
    &&& forall |other: int| #![trigger work_has_candidate(state, other)]
        0 <= other < state.spec_work().len() && work_has_candidate(state, other) ==>
            !work_precedes(state, other, work_index)
}

/// No retained work/worker pair is feasible.
pub open spec fn no_dispatch_candidate(state: &SchedulerState) -> bool {
    forall |work_index: int| 0 <= work_index < state.spec_work().len() ==>
        !work_has_candidate(state, work_index)
}

/// No candidate exists before the exclusive retained-work bound.
pub open spec fn no_candidate_before(state: &SchedulerState, end: int) -> bool {
    forall |work_index: int| 0 <= work_index < end ==>
        !work_has_candidate(state, work_index)
}

/// Exact best candidate among the retained-work prefix.
pub open spec fn chosen_prefix_matches(
    state: &SchedulerState,
    end: int,
    work_index: int,
    worker_index: int,
) -> bool {
    &&& 0 <= work_index < end
    &&& first_candidate_worker(state, work_index, worker_index)
    &&& forall |other: int| #![trigger work_has_candidate(state, other)]
        0 <= other < end && work_has_candidate(state, other) ==>
            !work_precedes(state, other, work_index)
}

pub(super) struct IndexedSelection {
    pub(super) work_index: usize,
    pub(super) worker_index: usize,
}

/// Relates a returned selection to an exact feasible retained pair.
pub open spec fn selection_is_feasible(state: &SchedulerState, selection: Selection) -> bool {
    exists |work_index: int, worker_index: int| #![auto]
        selected_pair_feasible(state, work_index, worker_index)
            && state.spec_work()[work_index].spec_definition().spec_id()
                == selection.spec_work_id()
            && state.spec_workers()[worker_index].spec_descriptor().spec_id()
                == selection.spec_worker_id()
}

/// Relates the public identity projection to the exact production choice.
pub open spec fn selection_is_exact(state: &SchedulerState, selection: Selection) -> bool {
    exists |work_index: int, worker_index: int|
        #![trigger chosen_pair_matches(state, work_index, worker_index)]
        chosen_pair_matches(state, work_index, worker_index)
            && state.spec_work()[work_index].spec_definition().spec_id()
                == selection.spec_work_id()
            && state.spec_workers()[worker_index].spec_descriptor().spec_id()
                == selection.spec_worker_id()
}

/// An admitted queued item cannot already own a reservation in a reducer-ready state.
pub(crate) proof fn admitted_work_is_unreserved(
    state: &SchedulerState,
    work_index: int,
    worker_index: int,
)
    requires
        state.spec_reservation_reducer_ready(),
        selected_pair_admitted(state, work_index, worker_index),
    ensures
        forall |reservation_index: int| #![auto]
            0 <= reservation_index < state.spec_reservations().len() ==>
                state.spec_reservations()[reservation_index].spec_work_id()
                    != state.spec_work()[work_index].spec_definition().spec_id(),
{
    reveal(selected_pair_admitted);
    reveal(crate::verified::reservation_invariant_parts);
    reveal(crate::verified::reservations_bind_active_work);
    reveal(crate::verified::work_identities_unique);
    reveal(crate::verified::work_phase_retains_reservation);
    assert forall |reservation_index: int| #![auto]
        0 <= reservation_index < state.spec_reservations().len()
        implies state.spec_reservations()[reservation_index].spec_work_id()
            != state.spec_work()[work_index].spec_definition().spec_id() by {
        if state.spec_reservations()[reservation_index].spec_work_id()
            == state.spec_work()[work_index].spec_definition().spec_id()
        {
            let active_index = choose |candidate: int|
                #![trigger state.spec_work()[candidate].spec_definition().spec_id()]
                0 <= candidate < state.spec_work().len()
                    && state.spec_work()[candidate].spec_definition().spec_id()
                        == state.spec_reservations()[reservation_index].spec_work_id()
                    && crate::verified::work_phase_retains_reservation(
                        state.spec_work()[candidate].spec_phase(),
                    );
            if active_index != work_index {
                assert(state.spec_work()[active_index].spec_definition().spec_id()
                    != state.spec_work()[work_index].spec_definition().spec_id());
                assert(false);
            }
            assert(state.spec_work()[work_index].spec_phase() == WorkPhase::Queued);
            assert(crate::verified::work_phase_retains_reservation(
                state.spec_work()[work_index].spec_phase(),
            ));
            assert(false);
        }
    }
}

} // verus!
