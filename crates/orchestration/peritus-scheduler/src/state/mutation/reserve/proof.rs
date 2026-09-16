//! Proof witnesses for the executable selected-reservation mutation.

#[cfg(verus_only)]
mod readiness;

#[cfg(verus_only)]
use crate::{
    DispatchId, SchedulerBinding, SchedulerReservation, SchedulerState, WorkRecord, WorkerRecord,
};
use vstd::prelude::*;

#[cfg(verus_only)]
pub use readiness::inserted_state_is_ready;

verus! {

pub(super) proof fn admitted_pair_has_no_live_ownership(
    state: &SchedulerState,
    work_index: int,
    worker_index: int,
    dispatch_id: DispatchId,
)
    requires
        state.spec_reservation_reducer_ready(),
        crate::selection::selected_pair_admitted(state, work_index, worker_index),
        !state.spec_used_dispatches().contains(dispatch_id),
    ensures
        forall |reservation_index: int| #![auto]
            0 <= reservation_index < state.spec_reservations().len() ==>
                state.spec_reservations()[reservation_index].spec_dispatch_id() != dispatch_id
                    && state.spec_reservations()[reservation_index].spec_work_id()
                        != state.spec_work()[work_index].spec_definition().spec_id(),
{
    crate::selection::admitted_work_is_unreserved(state, work_index, worker_index);
    crate::verified::unused_dispatch_is_not_live(state, dispatch_id);
}

pub open spec fn feasibility_context(
    state: &SchedulerState,
    before_binding: &SchedulerBinding,
    before_workers: Seq<WorkerRecord>,
    before_work: Seq<WorkRecord>,
    before_reservations: Seq<SchedulerReservation>,
    work_index: int,
    worker_index: int,
    dispatch_id: DispatchId,
    reservation: &SchedulerReservation,
) -> bool {
    &&& crate::selection::selected_pair_feasible_parts(
        before_binding,
        before_workers,
        before_work,
        before_reservations,
        work_index,
        worker_index,
    )
    &&& state.spec_reservation_invariant()
    &&& state.spec_binding().spec_limits() == before_binding.spec_limits()
    &&& state.spec_binding().spec_capacity().spec_entries()
        == before_binding.spec_capacity().spec_entries()
    &&& before_reservations.len()
        < before_binding.spec_limits().spec_active_reservations()
    &&& state.spec_workers() == before_workers
    &&& state.spec_reservations() == before_reservations
    &&& before_work.len() == state.spec_work().len()
    &&& WorkRecord::reservation_subject_equivalent(
        &before_work[work_index],
        &state.spec_work()[work_index],
    )
    &&& forall |other: int| #![auto]
        0 <= other < before_work.len() && other != work_index ==>
            before_work[other] == state.spec_work()[other]
    &&& reservation.spec_work_id()
        == before_work[work_index].spec_definition().spec_id()
    &&& reservation.spec_dispatch_id() == dispatch_id
    &&& reservation.spec_worker_id()
        == before_workers[worker_index].spec_descriptor().spec_id()
    &&& crate::identity::actor_ids_match(
        reservation.spec_owner(),
        before_work[work_index].spec_definition().spec_owner(),
    )
    &&& reservation.spec_revision()
        == before_work[work_index].spec_definition().spec_revision()
    &&& reservation.spec_resources().spec_entries()
        == before_work[work_index]
            .spec_definition().spec_request().spec_entries()
    &&& reservation.spec_attempt().spec_value()
        == state.spec_work()[work_index].spec_attempts_started()
    &&& forall |index: int| #![auto]
        0 <= index < before_reservations.len() ==>
            before_reservations[index].spec_dispatch_id() != dispatch_id
                && before_reservations[index].spec_work_id()
                    != before_work[work_index].spec_definition().spec_id()
}

proof fn prove_worker_witness(
    state: &SchedulerState,
    before_binding: &SchedulerBinding,
    before_workers: Seq<WorkerRecord>,
    before_work: Seq<WorkRecord>,
    before_reservations: Seq<SchedulerReservation>,
    work_index: int,
    worker_index: int,
    dispatch_id: DispatchId,
    reservation: &SchedulerReservation,
)
    requires feasibility_context(
        state, before_binding, before_workers, before_work, before_reservations,
        work_index, worker_index, dispatch_id, reservation,
    ),
    ensures exists |selected_worker: int| #![auto]
        0 <= selected_worker < state.spec_workers().len()
            && state.spec_workers()[selected_worker].spec_descriptor().spec_id()
                == reservation.spec_worker_id()
            && crate::identity::actor_ids_match(
                state.spec_workers()[selected_worker].spec_descriptor().spec_owner(),
                reservation.spec_owner(),
            )
            && crate::verified::worker_count(
                state.spec_reservations(), reservation.spec_worker_id(),
            ) < state.spec_workers()[selected_worker].spec_descriptor().spec_concurrency()
            && forall |kind: crate::ResourceKind| #![auto]
                crate::verified::worker_quantity(
                    state.spec_reservations(), reservation.spec_worker_id(), kind,
                ) + crate::verified::vector_quantity(
                    reservation.spec_resources().spec_entries(), kind,
                ) <= crate::verified::vector_quantity(
                    state.spec_workers()[selected_worker]
                        .spec_descriptor().spec_capacity().spec_entries(),
                    kind,
                ),
{
    reveal(feasibility_context);
    reveal(crate::selection::selected_pair_feasible_parts);
    assert(crate::identity::actor_ids_match(
        before_workers[worker_index].spec_descriptor().spec_owner(),
        before_work[work_index].spec_definition().spec_owner(),
    ));
    reveal(crate::identity::actor_ids_match);
    assert(crate::identity::actor_ids_match(
        state.spec_workers()[worker_index].spec_descriptor().spec_owner(),
        reservation.spec_owner(),
    ));
    assert forall |kind: crate::ResourceKind| #![auto]
        crate::verified::worker_quantity(
            state.spec_reservations(), reservation.spec_worker_id(), kind,
        ) + crate::verified::vector_quantity(
            reservation.spec_resources().spec_entries(), kind,
        ) <= crate::verified::vector_quantity(
            state.spec_workers()[worker_index]
                .spec_descriptor().spec_capacity().spec_entries(),
            kind,
        ) by {
        before_workers[worker_index]
            .spec_descriptor().spec_capacity().quantity_matches_entries(kind);
        state.spec_workers()[worker_index]
            .spec_descriptor().spec_capacity().quantity_matches_entries(kind);
    }
    assert(exists |selected_worker: int| #![auto]
        0 <= selected_worker < state.spec_workers().len()
            && state.spec_workers()[selected_worker].spec_descriptor().spec_id()
                == reservation.spec_worker_id()
            && crate::identity::actor_ids_match(
                state.spec_workers()[selected_worker].spec_descriptor().spec_owner(),
                reservation.spec_owner(),
            )
            && crate::verified::worker_count(
                state.spec_reservations(), reservation.spec_worker_id(),
            ) < state.spec_workers()[selected_worker].spec_descriptor().spec_concurrency()
            && forall |kind: crate::ResourceKind| #![auto]
                crate::verified::worker_quantity(
                    state.spec_reservations(), reservation.spec_worker_id(), kind,
                ) + crate::verified::vector_quantity(
                    reservation.spec_resources().spec_entries(), kind,
                ) <= crate::verified::vector_quantity(
                    state.spec_workers()[selected_worker]
                        .spec_descriptor().spec_capacity().spec_entries(),
                    kind,
                )) by {
    }
}

proof fn prove_global_capacity(
    state: &SchedulerState,
    before_binding: &SchedulerBinding,
    before_workers: Seq<WorkerRecord>,
    before_work: Seq<WorkRecord>,
    before_reservations: Seq<SchedulerReservation>,
    work_index: int,
    worker_index: int,
    dispatch_id: DispatchId,
    reservation: &SchedulerReservation,
)
    requires feasibility_context(
        state, before_binding, before_workers, before_work, before_reservations,
        work_index, worker_index, dispatch_id, reservation,
    ),
    ensures forall |kind: crate::ResourceKind| #![auto]
        crate::verified::reservation_quantity(state.spec_reservations(), kind)
                + crate::verified::vector_quantity(
                    reservation.spec_resources().spec_entries(), kind,
                )
            <= crate::verified::vector_quantity(
                state.spec_binding().spec_capacity().spec_entries(), kind,
            ),
{
    reveal(feasibility_context);
    reveal(crate::selection::selected_pair_feasible_parts);
    assert forall |kind: crate::ResourceKind| #![auto]
        crate::verified::reservation_quantity(state.spec_reservations(), kind)
                + crate::verified::vector_quantity(
                    reservation.spec_resources().spec_entries(), kind,
                )
            <= crate::verified::vector_quantity(
                state.spec_binding().spec_capacity().spec_entries(), kind,
            ) by {
        before_binding.spec_capacity().quantity_matches_entries(kind);
        state.spec_binding().spec_capacity().quantity_matches_entries(kind);
    }
}

proof fn selected_reservation_is_feasible(
    state: &SchedulerState,
    before_binding: &SchedulerBinding,
    before_workers: Seq<WorkerRecord>,
    before_work: Seq<WorkRecord>,
    before_reservations: Seq<SchedulerReservation>,
    work_index: int,
    worker_index: int,
    dispatch_id: DispatchId,
    reservation: &SchedulerReservation,
)
    requires feasibility_context(
        state, before_binding, before_workers, before_work, before_reservations,
        work_index, worker_index, dispatch_id, reservation,
    ),
    ensures state.spec_reservation_feasible(reservation),
{
    reveal(feasibility_context);
    reveal(crate::verified::reservation_feasible_parts);
    assert forall |index: int| #![auto]
        0 <= index < state.spec_reservations().len()
        implies state.spec_reservations()[index].spec_dispatch_id()
                != reservation.spec_dispatch_id()
            && state.spec_reservations()[index].spec_work_id()
                != reservation.spec_work_id() by {
    }
    prove_worker_witness(
        state, before_binding, before_workers, before_work, before_reservations,
        work_index, worker_index, dispatch_id, reservation,
    );
    WorkRecord::reservation_subject_fields(
        &before_work[work_index],
        &state.spec_work()[work_index],
    );
    assert(exists |selected_work: int| #![auto]
        0 <= selected_work < state.spec_work().len()
            && state.spec_work()[selected_work].spec_definition().spec_id()
                == reservation.spec_work_id()
            && crate::identity::actor_ids_match(
                state.spec_work()[selected_work].spec_definition().spec_owner(),
                reservation.spec_owner(),
            )
            && state.spec_work()[selected_work].spec_definition().spec_revision()
                == reservation.spec_revision()
            && state.spec_work()[selected_work]
                .spec_definition().spec_request().spec_entries()
                == reservation.spec_resources().spec_entries()
            && state.spec_work()[selected_work].spec_attempts_started()
                == reservation.spec_attempt().spec_value()) by {
        reveal(crate::identity::actor_ids_match);
    }
    prove_global_capacity(
        state, before_binding, before_workers, before_work, before_reservations,
        work_index, worker_index, dispatch_id, reservation,
    );
}

pub(super) proof fn admitted_reservation_is_feasible(
    state: &SchedulerState,
    before_binding: &SchedulerBinding,
    before_workers: Seq<WorkerRecord>,
    before_work: Seq<WorkRecord>,
    before_reservations: Seq<SchedulerReservation>,
    work_index: int,
    worker_index: int,
    dispatch_id: DispatchId,
    reservation: &SchedulerReservation,
    stored: &SchedulerReservation,
)
    requires
        crate::selection::selected_pair_feasible_parts(
            before_binding,
            before_workers,
            before_work,
            before_reservations,
            work_index,
            worker_index,
        ),
        state.spec_reservation_invariant(),
        state.spec_binding().spec_limits() == before_binding.spec_limits(),
        state.spec_binding().spec_capacity().spec_entries()
            == before_binding.spec_capacity().spec_entries(),
        before_reservations.len()
            < before_binding.spec_limits().spec_active_reservations(),
        state.spec_workers() == before_workers,
        state.spec_reservations() == before_reservations,
        before_work.len() == state.spec_work().len(),
        WorkRecord::reservation_subject_equivalent(
            &before_work[work_index],
            &state.spec_work()[work_index],
        ),
        forall |other: int| #![auto]
            0 <= other < before_work.len() && other != work_index ==>
                before_work[other] == state.spec_work()[other],
        forall |index: int| #![auto]
            0 <= index < before_reservations.len() ==>
                before_reservations[index].spec_dispatch_id() != dispatch_id
                    && before_reservations[index].spec_work_id()
                        != before_work[work_index].spec_definition().spec_id(),
        reservation.spec_work_id()
            == before_work[work_index].spec_definition().spec_id(),
        reservation.spec_dispatch_id() == dispatch_id,
        reservation.spec_worker_id()
            == before_workers[worker_index].spec_descriptor().spec_id(),
        crate::identity::actor_ids_match(
            reservation.spec_owner(),
            before_work[work_index].spec_definition().spec_owner(),
        ),
        reservation.spec_revision()
            == before_work[work_index].spec_definition().spec_revision(),
        reservation.spec_resources().spec_entries()
            == before_work[work_index].spec_definition().spec_request().spec_entries(),
        reservation.spec_attempt().spec_value()
            == state.spec_work()[work_index].spec_attempts_started(),
        SchedulerReservation::clone_equivalent(reservation, stored),
    ensures
        state.spec_reservation_feasible(stored),
        stored.spec_dispatch_id() == dispatch_id,
        stored.spec_work_id()
            == before_work[work_index].spec_definition().spec_id(),
{
    SchedulerReservation::clone_fields(reservation, stored);
    WorkRecord::reservation_subject_fields(
        &before_work[work_index],
        &state.spec_work()[work_index],
    );
    assert(feasibility_context(
        state,
        before_binding,
        before_workers,
        before_work,
        before_reservations,
        work_index,
        worker_index,
        dispatch_id,
        stored,
    ));
    selected_reservation_is_feasible(
        state,
        before_binding,
        before_workers,
        before_work,
        before_reservations,
        work_index,
        worker_index,
        dispatch_id,
        stored,
    );
}

} // verus!
