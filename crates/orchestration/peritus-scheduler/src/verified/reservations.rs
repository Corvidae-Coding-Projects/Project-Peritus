//! Actual-state reservation capacity and ownership model for OBL-0149.

use crate::SchedulerState;
use vstd::prelude::*;

#[cfg(verus_only)]
use crate::{ResourceEntry, ResourceKind, SchedulerBinding, SchedulerReservation, WorkerRecord};

mod arithmetic;
mod clone;
mod entity_insertion;
mod feasibility;
mod insertion;
mod preservation;
mod readiness;
mod removal;
mod work_update;
mod worker_update;

#[cfg(verus_only)]
pub(crate) use clone::equivalent_state_preserves;
#[cfg(verus_only)]
pub(crate) use preservation::vector_quantity_nonnegative;
#[cfg(verus_only)]
pub(super) use preservation::{
    insertion_preserves, removal_preserves, unreserved_work_update_preserves,
    work_insertion_preserves, work_update_preserves, worker_insertion_preserves,
    worker_update_preserves,
};
#[cfg(verus_only)]
pub(crate) use readiness::{
    active_work_has_reservation, active_work_has_reservations, active_work_has_reservations_intro,
    admissible_work_update_preserves_relations, equivalent_sequences_preserve_phase,
    inactive_work_insertion_preserves_phase, inactive_work_update_is_admissible,
    release_preserves_relations, reservation_has_active_work, reservation_matches_work_phase,
    reservations_match_work_phases, selected_insertion_establishes_phase,
    start_acknowledgement_preserves_relations, work_phase_update_admissible,
};

verus! {

/// Returns one resource dimension's exact mathematical quantity.
pub open spec fn vector_quantity(entries: Seq<ResourceEntry>, kind: ResourceKind) -> int
{
    crate::resource::capacity::entries_quantity(entries, kind)
}

/// Returns the exact global live-reservation quantity for one dimension.
pub open spec fn reservation_quantity(
    reservations: Seq<SchedulerReservation>,
    kind: ResourceKind,
) -> int
    decreases reservations.len(),
{
    if reservations.len() == 0 {
        0
    } else {
        vector_quantity(reservations.first().spec_resources().spec_entries(), kind)
            + reservation_quantity(reservations.drop_first(), kind)
    }
}

/// Returns the exact worker-local live-reservation quantity for one dimension.
pub open spec fn worker_quantity(
    reservations: Seq<SchedulerReservation>,
    worker: crate::WorkerId,
    kind: ResourceKind,
) -> int
    decreases reservations.len(),
{
    if reservations.len() == 0 {
        0
    } else {
        let head = reservations.first();
        (if head.spec_worker_id() == worker {
            vector_quantity(head.spec_resources().spec_entries(), kind)
        } else {
            0
        }) + worker_quantity(reservations.drop_first(), worker, kind)
    }
}

/// Returns the exact number of live reservations owned by one worker.
pub open spec fn worker_count(
    reservations: Seq<SchedulerReservation>,
    worker: crate::WorkerId,
) -> int
    decreases reservations.len(),
{
    if reservations.len() == 0 {
        0
    } else {
        (if reservations.first().spec_worker_id() == worker { 1int } else { 0int })
            + worker_count(reservations.drop_first(), worker)
    }
}

/// Returns whether dispatch and work ownership identities are each unique.
pub open spec fn reservation_identities_unique(
    reservations: Seq<SchedulerReservation>,
) -> bool {
    forall |left: int, right: int| #![auto]
        0 <= left < reservations.len()
            && 0 <= right < reservations.len()
            && left != right
            ==> reservations[left].spec_dispatch_id() != reservations[right].spec_dispatch_id()
                && reservations[left].spec_work_id() != reservations[right].spec_work_id()
}

/// Returns whether retained worker identities select at most one descriptor.
pub open spec fn worker_identities_unique(workers: Seq<WorkerRecord>) -> bool {
    forall |left: int, right: int| #![auto]
        0 <= left < workers.len()
            && 0 <= right < workers.len()
            && left != right
            ==> workers[left].spec_descriptor().spec_id()
                != workers[right].spec_descriptor().spec_id()
}

/// Returns whether one worker identity is absent from retained state.
pub open spec fn worker_identity_absent(
    workers: Seq<WorkerRecord>,
    id: crate::WorkerId,
) -> bool {
    forall |index: int| #![auto]
        0 <= index < workers.len() ==>
            workers[index].spec_descriptor().spec_id() != id
}

/// Returns whether retained work identities select at most one work definition.
pub open spec fn work_identities_unique(work: Seq<crate::WorkRecord>) -> bool {
    forall |left: int, right: int| #![auto]
        0 <= left < work.len()
            && 0 <= right < work.len()
            && left != right
            ==> work[left].spec_definition().spec_id()
                != work[right].spec_definition().spec_id()
}

/// Returns whether one work identity is absent from retained state.
pub open spec fn work_identity_absent(
    work: Seq<crate::WorkRecord>,
    id: crate::WorkId,
) -> bool {
    forall |index: int| #![auto]
        0 <= index < work.len() ==>
            work[index].spec_definition().spec_id() != id
}

/// Returns whether every reservation names exactly one retained owner worker.
pub open spec fn reservations_have_workers(
    workers: Seq<WorkerRecord>,
    reservations: Seq<SchedulerReservation>,
) -> bool {
    forall |reservation_index: int| #![trigger reservations[reservation_index]]
        0 <= reservation_index < reservations.len()
            ==> exists |worker_index: int|
                #![trigger workers[worker_index].spec_descriptor().spec_id()]
                0 <= worker_index < workers.len()
                    && workers[worker_index].spec_descriptor().spec_id()
                        == reservations[reservation_index].spec_worker_id()
                    && crate::identity::actor_ids_match(
                        workers[worker_index].spec_descriptor().spec_owner(),
                        reservations[reservation_index].spec_owner(),
                    )
}

/// Returns whether one retained work attempt exactly owns a reservation.
pub open spec fn work_binds_reservation(
    work: &crate::WorkRecord,
    reservation: &SchedulerReservation,
) -> bool {
    &&& work.spec_definition().spec_id() == reservation.spec_work_id()
    &&& crate::identity::actor_ids_match(
        work.spec_definition().spec_owner(),
        reservation.spec_owner(),
    )
    &&& work.spec_definition().spec_revision() == reservation.spec_revision()
    &&& work.spec_definition().spec_request().spec_entries()
        == reservation.spec_resources().spec_entries()
    &&& work.spec_attempts_started() == reservation.spec_attempt().spec_value()
}

/// Returns whether one reservation has an exact retained work witness.
pub open spec fn reservation_has_work(
    work: Seq<crate::WorkRecord>,
    reservation: &SchedulerReservation,
) -> bool {
    exists |work_index: int|
        #![trigger work[work_index].spec_definition().spec_id()]
        0 <= work_index < work.len()
            && work_binds_reservation(&work[work_index], reservation)
}

/// Returns whether every reservation names one exact retained work attempt.
pub open spec fn reservations_have_work(
    work: Seq<crate::WorkRecord>,
    reservations: Seq<SchedulerReservation>,
) -> bool {
    forall |reservation_index: int| #![trigger reservations[reservation_index]]
        0 <= reservation_index < reservations.len()
            ==> reservation_has_work(work, &reservations[reservation_index])
}

pub(super) proof fn reservations_have_work_intro(
    work: Seq<crate::WorkRecord>,
    reservations: Seq<SchedulerReservation>,
)
    requires forall |reservation_index: int| #![trigger reservations[reservation_index]]
        0 <= reservation_index < reservations.len() ==>
            reservation_has_work(work, &reservations[reservation_index]),
    ensures reservations_have_work(work, reservations),
{
    reveal(reservations_have_work);
}

/// Actual binding, workers, and reservation sequence satisfy every OBL-0149 capacity bound.
pub open spec fn reservation_invariant_parts(
    binding: &SchedulerBinding,
    workers: Seq<WorkerRecord>,
    work: Seq<crate::WorkRecord>,
    reservations: Seq<SchedulerReservation>,
) -> bool {
    &&& reservations.len() <= binding.spec_limits().spec_active_reservations()
    &&& worker_identities_unique(workers)
    &&& work_identities_unique(work)
    &&& reservation_identities_unique(reservations)
    &&& reservations_have_workers(workers, reservations)
    &&& reservations_have_work(work, reservations)
    &&& forall |kind: ResourceKind|
        #![trigger reservation_quantity(reservations, kind)]
        0 <= reservation_quantity(reservations, kind)
            <= vector_quantity(binding.spec_capacity().spec_entries(), kind)
    &&& forall |worker_index: int|
        #![trigger workers[worker_index].spec_descriptor().spec_id()]
        0 <= worker_index < workers.len() ==> {
            let descriptor = workers[worker_index].spec_descriptor();
            &&& 0 <= worker_count(reservations, descriptor.spec_id())
                <= descriptor.spec_concurrency()
            &&& forall |kind: ResourceKind|
                #![trigger worker_quantity(reservations, descriptor.spec_id(), kind)]
                0 <= worker_quantity(reservations, descriptor.spec_id(), kind)
                    <= vector_quantity(descriptor.spec_capacity().spec_entries(), kind)
        }
}

/// Actual scheduler-state reservation invariant used by the production reducer proof.
pub closed spec fn reservation_invariant(state: &SchedulerState) -> bool {
    reservation_invariant_parts(
        state.spec_binding(),
        state.spec_workers(),
        state.spec_work(),
        state.spec_reservations(),
    )
}

impl SchedulerState {
    /// Returns the actual-state OBL-0149 reservation invariant.
    pub open spec fn spec_reservation_invariant(&self) -> bool {
        reservation_invariant_parts(
            self.spec_binding(),
            self.spec_workers(),
            self.spec_work(),
            self.spec_reservations(),
        )
    }

}

#[cfg(verus_only)]
pub(crate) use feasibility::reservation_feasible_parts;

#[cfg(verus_only)]
pub(crate) use readiness::{
    lifecycle_work_update_preserves_relations,
    reservations_are_retained_dispatches, reservations_bind_active_work,
    selected_insertion_establishes_relations, unused_dispatch_is_not_live,
    work_phase_retains_reservation,
};


} // verus!
