//! Public preservation adapters for the OBL-0149 reservation model.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::{ResourceEntry, ResourceKind, SchedulerBinding, SchedulerReservation, WorkerRecord};

#[cfg(verus_only)]
use super::{
    arithmetic, entity_insertion, insertion, removal, reservation_feasible_parts,
    reservation_invariant_parts, vector_quantity, work_identity_absent, work_update,
    worker_identity_absent, worker_update,
};

verus! {

pub(crate) proof fn insertion_preserves(
    binding: &SchedulerBinding,
    workers: Seq<WorkerRecord>,
    work: Seq<crate::WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    reservation: SchedulerReservation,
    at: int,
)
    requires
        reservation_invariant_parts(binding, workers, work, reservations),
        reservation_feasible_parts(binding, workers, work, reservations, &reservation),
        0 <= at <= reservations.len(),
    ensures
        reservation_invariant_parts(
            binding,
            workers,
            work,
            reservations.insert(at, reservation),
        ),
{
    reveal(reservation_feasible_parts);
    insertion::feasible_insertion_preserves(
        binding,
        workers,
        work,
        reservations,
        reservation,
        at,
    );
}

pub(crate) proof fn vector_quantity_nonnegative(
    entries: Seq<ResourceEntry>,
    kind: ResourceKind,
)
    ensures 0 <= vector_quantity(entries, kind),
{
    arithmetic::vector_quantity_nonnegative(entries, kind);
}

pub(crate) proof fn removal_preserves(
    binding: &SchedulerBinding,
    workers: Seq<WorkerRecord>,
    work: Seq<crate::WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    at: int,
)
    requires
        reservation_invariant_parts(binding, workers, work, reservations),
        0 <= at < reservations.len(),
    ensures
        reservation_invariant_parts(binding, workers, work, reservations.remove(at)),
{
    removal::exact_removal_preserves(binding, workers, work, reservations, at);
}

pub(crate) proof fn work_update_preserves(
    binding: &SchedulerBinding,
    workers: Seq<WorkerRecord>,
    before: Seq<crate::WorkRecord>,
    after: Seq<crate::WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    at: int,
)
    requires
        reservation_invariant_parts(binding, workers, before, reservations),
        before.len() == after.len(),
        0 <= at < before.len(),
        crate::WorkRecord::reservation_binding_equivalent(&before[at], &after[at]),
        forall |index: int| #![auto]
            0 <= index < before.len() && index != at ==> before[index] == after[index],
    ensures
        reservation_invariant_parts(binding, workers, after, reservations),
{
    work_update::stable_work_update_preserves(
        binding,
        workers,
        before,
        after,
        reservations,
        at,
    );
}

pub(crate) proof fn unreserved_work_update_preserves(
    binding: &SchedulerBinding,
    workers: Seq<WorkerRecord>,
    before: Seq<crate::WorkRecord>,
    after: Seq<crate::WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    at: int,
)
    requires
        reservation_invariant_parts(binding, workers, before, reservations),
        before.len() == after.len(),
        0 <= at < before.len(),
        crate::WorkRecord::reservation_subject_equivalent(&before[at], &after[at]),
        forall |index: int| #![auto]
            0 <= index < before.len() && index != at ==> before[index] == after[index],
        forall |reservation_index: int| #![auto]
            0 <= reservation_index < reservations.len() ==>
                reservations[reservation_index].spec_work_id()
                    != before[at].spec_definition().spec_id(),
    ensures reservation_invariant_parts(binding, workers, after, reservations),
{
    work_update::unreserved_work_update_preserves(
        binding,
        workers,
        before,
        after,
        reservations,
        at,
    );
}

pub(crate) proof fn worker_update_preserves(
    binding: &SchedulerBinding,
    before: Seq<WorkerRecord>,
    after: Seq<WorkerRecord>,
    work: Seq<crate::WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    at: int,
)
    requires
        reservation_invariant_parts(binding, before, work, reservations),
        before.len() == after.len(),
        0 <= at < before.len(),
        WorkerRecord::reservation_owner_equivalent(&before[at], &after[at]),
        forall |index: int| #![auto]
            0 <= index < before.len() && index != at ==> before[index] == after[index],
    ensures
        reservation_invariant_parts(binding, after, work, reservations),
{
    worker_update::stable_worker_update_preserves(
        binding,
        before,
        after,
        work,
        reservations,
        at,
    );
}

pub(crate) proof fn worker_insertion_preserves(
    binding: &SchedulerBinding,
    workers: Seq<WorkerRecord>,
    work: Seq<crate::WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    worker: WorkerRecord,
    at: int,
)
    requires
        reservation_invariant_parts(binding, workers, work, reservations),
        0 <= at <= workers.len(),
        worker_identity_absent(workers, worker.spec_descriptor().spec_id()),
    ensures reservation_invariant_parts(
        binding,
        workers.insert(at, worker),
        work,
        reservations,
    ),
{
    entity_insertion::worker_insertion_preserves(
        binding, workers, work, reservations, worker, at,
    );
}

pub(crate) proof fn work_insertion_preserves(
    binding: &SchedulerBinding,
    workers: Seq<WorkerRecord>,
    work: Seq<crate::WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    record: crate::WorkRecord,
    at: int,
)
    requires
        reservation_invariant_parts(binding, workers, work, reservations),
        0 <= at <= work.len(),
        work_identity_absent(work, record.spec_definition().spec_id()),
    ensures reservation_invariant_parts(
        binding,
        workers,
        work.insert(at, record),
        reservations,
    ),
{
    entity_insertion::work_insertion_preserves(
        binding, workers, work, reservations, record, at,
    );
}

} // verus!
