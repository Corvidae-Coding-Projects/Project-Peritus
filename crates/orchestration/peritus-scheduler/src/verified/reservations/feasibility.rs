//! Exact admission relation for one new durable reservation.

use crate::SchedulerState;
use vstd::prelude::*;

#[cfg(verus_only)]
use super::{reservation_quantity, vector_quantity, worker_count, worker_quantity};
#[cfg(verus_only)]
use crate::{ResourceKind, SchedulerBinding, SchedulerReservation, WorkerRecord};

verus! {

impl SchedulerState {
    /// Returns whether one actual reservation is admissible in this exact state.
    pub open spec fn spec_reservation_feasible(
        &self,
        reservation: &SchedulerReservation,
    ) -> bool {
        reservation_feasible_parts(
            self.spec_binding(),
            self.spec_workers(),
            self.spec_work(),
            self.spec_reservations(),
            reservation,
        )
    }
}

/// Exact feasibility needed to add one actual durable reservation.
pub open spec fn reservation_feasible(
    state: &SchedulerState,
    reservation: &SchedulerReservation,
) -> bool {
    reservation_feasible_parts(
        state.spec_binding(),
        state.spec_workers(),
        state.spec_work(),
        state.spec_reservations(),
        reservation,
    )
}

/// Exact feasibility over the reservation-relevant actual state fields.
pub open spec fn reservation_feasible_parts(
    binding: &SchedulerBinding,
    workers: Seq<WorkerRecord>,
    work: Seq<crate::WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    reservation: &SchedulerReservation,
) -> bool {
    &&& reservations.len() < binding.spec_limits().spec_active_reservations()
    &&& forall |index: int| #![auto] 0 <= index < reservations.len() ==>
        reservations[index].spec_dispatch_id() != reservation.spec_dispatch_id()
            && reservations[index].spec_work_id() != reservation.spec_work_id()
    &&& exists |worker_index: int| #![auto]
        0 <= worker_index < workers.len()
            && workers[worker_index].spec_descriptor().spec_id() == reservation.spec_worker_id()
            && crate::identity::actor_ids_match(
                workers[worker_index].spec_descriptor().spec_owner(),
                reservation.spec_owner(),
            )
            && worker_count(reservations, reservation.spec_worker_id())
                < workers[worker_index].spec_descriptor().spec_concurrency()
            && forall |kind: ResourceKind|
                #![trigger worker_quantity(reservations, reservation.spec_worker_id(), kind)]
                worker_quantity(reservations, reservation.spec_worker_id(), kind)
                        + vector_quantity(reservation.spec_resources().spec_entries(), kind)
                    <= vector_quantity(
                        workers[worker_index].spec_descriptor().spec_capacity().spec_entries(),
                        kind,
                    )
    &&& exists |work_index: int| #![auto]
        0 <= work_index < work.len()
            && work[work_index].spec_definition().spec_id() == reservation.spec_work_id()
            && crate::identity::actor_ids_match(
                work[work_index].spec_definition().spec_owner(),
                reservation.spec_owner(),
            )
            && work[work_index].spec_definition().spec_revision() == reservation.spec_revision()
            && work[work_index].spec_definition().spec_request().spec_entries()
                == reservation.spec_resources().spec_entries()
            && work[work_index].spec_attempts_started()
                == reservation.spec_attempt().spec_value()
    &&& forall |kind: ResourceKind|
        #![trigger reservation_quantity(reservations, kind)]
        reservation_quantity(reservations, kind)
                + vector_quantity(reservation.spec_resources().spec_entries(), kind)
            <= vector_quantity(binding.spec_capacity().spec_entries(), kind)
}

} // verus!
