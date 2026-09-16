//! Reservation-invariant transport across the actual semantic state clone.

use vstd::prelude::*;

mod arithmetic;

#[cfg(verus_only)]
use crate::{
    DispatchId, ResourceKind, SchedulerBinding, SchedulerReservation, WorkRecord, WorkerRecord,
};

#[cfg(verus_only)]
use super::{
    reservation_identities_unique, reservation_invariant_parts, reservation_quantity,
    reservations_are_retained_dispatches, reservations_bind_active_work, reservations_have_work,
    reservations_have_workers, vector_quantity, work_identities_unique,
    work_phase_retains_reservation, worker_count, worker_identities_unique, worker_quantity,
};

#[cfg(verus_only)]
use arithmetic::{reservation_quantities_match, worker_counts_match, worker_quantities_match};

verus! {
pub(crate) proof fn equivalent_state_preserves(
    before_binding: &SchedulerBinding,
    after_binding: &SchedulerBinding,
    before_workers: Seq<WorkerRecord>,
    after_workers: Seq<WorkerRecord>,
    before_work: Seq<WorkRecord>,
    after_work: Seq<WorkRecord>,
    before_reservations: Seq<SchedulerReservation>,
    after_reservations: Seq<SchedulerReservation>,
    before_used: Seq<DispatchId>,
    after_used: Seq<DispatchId>,
)
    requires
        before_binding.spec_limits() == after_binding.spec_limits(),
        before_binding.spec_capacity().spec_entries()
            == after_binding.spec_capacity().spec_entries(),
        before_workers.len() == after_workers.len(),
        forall |index: int| #![auto]
            0 <= index < before_workers.len() ==>
                WorkerRecord::reservation_owner_equivalent(
                    &before_workers[index],
                    &after_workers[index],
                ),
        before_work.len() == after_work.len(),
        forall |index: int| #![auto]
            0 <= index < before_work.len() ==>
                WorkRecord::reservation_lifecycle_equivalent(
                    &before_work[index],
                    &after_work[index],
                ),
        before_reservations.len() == after_reservations.len(),
        forall |index: int| #![auto]
            0 <= index < before_reservations.len() ==>
                SchedulerReservation::invariant_equivalent(
                    &before_reservations[index],
                    &after_reservations[index],
                ),
        before_used == after_used,
        reservation_invariant_parts(
            before_binding,
            before_workers,
            before_work,
            before_reservations,
        ),
        reservations_bind_active_work(before_work, before_reservations),
        reservations_are_retained_dispatches(before_reservations, before_used),
    ensures
        reservation_invariant_parts(
            after_binding,
            after_workers,
            after_work,
            after_reservations,
        ),
        reservations_bind_active_work(after_work, after_reservations),
        reservations_are_retained_dispatches(after_reservations, after_used),
{
    reveal(reservation_invariant_parts);
    reveal(worker_identities_unique);
    reveal(work_identities_unique);
    reveal(reservation_identities_unique);
    reveal(reservations_have_workers);
    reveal(reservations_have_work);

    assert(worker_identities_unique(after_workers)) by {
        assert forall |left: int, right: int| #![auto]
            0 <= left < after_workers.len()
                && 0 <= right < after_workers.len()
                && left != right
            implies after_workers[left].spec_descriptor().spec_id()
                != after_workers[right].spec_descriptor().spec_id() by {
            WorkerRecord::reservation_owner_fields(
                &before_workers[left],
                &after_workers[left],
            );
            WorkerRecord::reservation_owner_fields(
                &before_workers[right],
                &after_workers[right],
            );
        }
    }
    assert(work_identities_unique(after_work)) by {
        assert forall |left: int, right: int| #![auto]
            0 <= left < after_work.len() && 0 <= right < after_work.len() && left != right
            implies after_work[left].spec_definition().spec_id()
                != after_work[right].spec_definition().spec_id() by {
            WorkRecord::reservation_lifecycle_fields(&before_work[left], &after_work[left]);
            WorkRecord::reservation_lifecycle_fields(&before_work[right], &after_work[right]);
        }
    }
    assert(reservation_identities_unique(after_reservations)) by {
        assert forall |left: int, right: int| #![auto]
            0 <= left < after_reservations.len()
                && 0 <= right < after_reservations.len()
                && left != right
            implies after_reservations[left].spec_dispatch_id()
                    != after_reservations[right].spec_dispatch_id()
                && after_reservations[left].spec_work_id()
                    != after_reservations[right].spec_work_id() by {
            SchedulerReservation::invariant_fields(
                &before_reservations[left],
                &after_reservations[left],
            );
            SchedulerReservation::invariant_fields(
                &before_reservations[right],
                &after_reservations[right],
            );
        }
    }
    assert(reservations_have_workers(after_workers, after_reservations)) by {
        assert forall |reservation_index: int| #![trigger after_reservations[reservation_index]]
            0 <= reservation_index < after_reservations.len()
            implies exists |worker_index: int|
                #![trigger after_workers[worker_index].spec_descriptor().spec_id()]
                0 <= worker_index < after_workers.len()
                    && after_workers[worker_index].spec_descriptor().spec_id()
                        == after_reservations[reservation_index].spec_worker_id()
                    && crate::identity::actor_ids_match(
                        after_workers[worker_index].spec_descriptor().spec_owner(),
                        after_reservations[reservation_index].spec_owner(),
                    ) by {
            SchedulerReservation::invariant_fields(
                &before_reservations[reservation_index],
                &after_reservations[reservation_index],
            );
            let worker_index = choose |index: int|
                #![trigger before_workers[index].spec_descriptor().spec_id()]
                0 <= index < before_workers.len()
                    && before_workers[index].spec_descriptor().spec_id()
                        == before_reservations[reservation_index].spec_worker_id()
                    && crate::identity::actor_ids_match(
                        before_workers[index].spec_descriptor().spec_owner(),
                        before_reservations[reservation_index].spec_owner(),
                    );
            WorkerRecord::reservation_owner_fields(
                &before_workers[worker_index],
                &after_workers[worker_index],
            );
        }
    }
    assert(reservations_have_work(after_work, after_reservations)) by {
        assert forall |reservation_index: int| #![trigger after_reservations[reservation_index]]
            0 <= reservation_index < after_reservations.len()
            implies exists |work_index: int|
                #![trigger after_work[work_index].spec_definition().spec_id()]
                0 <= work_index < after_work.len()
                    && after_work[work_index].spec_definition().spec_id()
                        == after_reservations[reservation_index].spec_work_id()
                    && crate::identity::actor_ids_match(
                        after_work[work_index].spec_definition().spec_owner(),
                        after_reservations[reservation_index].spec_owner(),
                    )
                    && after_work[work_index].spec_definition().spec_revision()
                        == after_reservations[reservation_index].spec_revision()
                    && after_work[work_index].spec_definition().spec_request().spec_entries()
                        == after_reservations[reservation_index].spec_resources().spec_entries()
                    && after_work[work_index].spec_attempts_started()
                        == after_reservations[reservation_index].spec_attempt().spec_value() by {
            SchedulerReservation::invariant_fields(
                &before_reservations[reservation_index],
                &after_reservations[reservation_index],
            );
            let work_index = choose |index: int|
                #![trigger before_work[index].spec_definition().spec_id()]
                0 <= index < before_work.len()
                    && before_work[index].spec_definition().spec_id()
                        == before_reservations[reservation_index].spec_work_id()
                    && crate::identity::actor_ids_match(
                        before_work[index].spec_definition().spec_owner(),
                        before_reservations[reservation_index].spec_owner(),
                    )
                    && before_work[index].spec_definition().spec_revision()
                        == before_reservations[reservation_index].spec_revision()
                    && before_work[index].spec_definition().spec_request().spec_entries()
                        == before_reservations[reservation_index].spec_resources().spec_entries()
                    && before_work[index].spec_attempts_started()
                        == before_reservations[reservation_index].spec_attempt().spec_value();
            WorkRecord::reservation_lifecycle_fields(
                &before_work[work_index],
                &after_work[work_index],
            );
            WorkRecord::reservation_binding_fields(
                &before_work[work_index],
                &after_work[work_index],
            );
        }
    }
    assert forall |kind: ResourceKind|
        #![trigger reservation_quantity(after_reservations, kind)]
        0 <= reservation_quantity(after_reservations, kind)
            <= vector_quantity(after_binding.spec_capacity().spec_entries(), kind) by {
        reservation_quantities_match(before_reservations, after_reservations, kind);
    }
    assert forall |worker_index: int|
        #![trigger after_workers[worker_index].spec_descriptor().spec_id()]
        0 <= worker_index < after_workers.len() implies {
            let descriptor = after_workers[worker_index].spec_descriptor();
            &&& 0 <= worker_count(after_reservations, descriptor.spec_id())
                <= descriptor.spec_concurrency()
            &&& forall |kind: ResourceKind|
                #![trigger worker_quantity(after_reservations, descriptor.spec_id(), kind)]
                0 <= worker_quantity(after_reservations, descriptor.spec_id(), kind)
                    <= vector_quantity(descriptor.spec_capacity().spec_entries(), kind)
        } by {
        WorkerRecord::reservation_owner_fields(
            &before_workers[worker_index],
            &after_workers[worker_index],
        );
        worker_counts_match(
            before_reservations,
            after_reservations,
            after_workers[worker_index].spec_descriptor().spec_id(),
        );
        assert forall |kind: ResourceKind|
            #![trigger worker_quantity(
                after_reservations,
                after_workers[worker_index].spec_descriptor().spec_id(),
                kind,
            )]
            0 <= worker_quantity(
                    after_reservations,
                    after_workers[worker_index].spec_descriptor().spec_id(),
                    kind,
                )
                <= vector_quantity(
                    after_workers[worker_index]
                        .spec_descriptor()
                        .spec_capacity()
                        .spec_entries(),
                    kind,
                ) by {
            worker_quantities_match(
                before_reservations,
                after_reservations,
                after_workers[worker_index].spec_descriptor().spec_id(),
                kind,
            );
        }
    }

    reveal(reservations_bind_active_work);
    assert forall |reservation_index: int| #![trigger after_reservations[reservation_index]]
        0 <= reservation_index < after_reservations.len()
        implies exists |work_index: int|
            #![trigger after_work[work_index].spec_definition().spec_id()]
            0 <= work_index < after_work.len()
                && after_work[work_index].spec_definition().spec_id()
                    == after_reservations[reservation_index].spec_work_id()
                && work_phase_retains_reservation(after_work[work_index].spec_phase()) by {
        SchedulerReservation::invariant_fields(
            &before_reservations[reservation_index],
            &after_reservations[reservation_index],
        );
        let work_index = choose |index: int|
            #![trigger before_work[index].spec_definition().spec_id()]
            0 <= index < before_work.len()
                && before_work[index].spec_definition().spec_id()
                    == before_reservations[reservation_index].spec_work_id()
                && work_phase_retains_reservation(before_work[index].spec_phase());
        WorkRecord::reservation_lifecycle_fields(&before_work[work_index], &after_work[work_index]);
    }

    reveal(reservations_are_retained_dispatches);
    assert forall |reservation_index: int| #![trigger after_reservations[reservation_index]]
        0 <= reservation_index < after_reservations.len()
        implies exists |used_index: int| #![trigger after_used[used_index]]
            0 <= used_index < after_used.len()
                && after_used[used_index]
                    == after_reservations[reservation_index].spec_dispatch_id() by {
        SchedulerReservation::invariant_fields(
            &before_reservations[reservation_index],
            &after_reservations[reservation_index],
        );
        let used_index = choose |index: int| #![trigger before_used[index]]
            0 <= index < before_used.len()
                && before_used[index]
                    == before_reservations[reservation_index].spec_dispatch_id();
        assert(after_used[used_index] == before_used[used_index]);
    }
}

} // verus!
