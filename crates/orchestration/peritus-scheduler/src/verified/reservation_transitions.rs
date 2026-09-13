//! Actual-state adapters for the OBL-0149 preservation lemmas.

use vstd::prelude::*;

#[cfg(verus_only)]
use super::reservations;

verus! {

pub(crate) proof fn actual_worker_insertion_preserves(
    binding: &crate::SchedulerBinding,
    workers: Seq<crate::WorkerRecord>,
    work: Seq<crate::WorkRecord>,
    reservations: Seq<crate::SchedulerReservation>,
    worker: crate::WorkerRecord,
    at: int,
)
    requires
        reservations::reservation_invariant_parts(binding, workers, work, reservations),
        0 <= at <= workers.len(),
        reservations::worker_identity_absent(
            workers,
            worker.spec_descriptor().spec_id(),
        ),
    ensures reservations::reservation_invariant_parts(
        binding,
        workers.insert(at, worker),
        work,
        reservations,
    ),
{
    reservations::worker_insertion_preserves(
        binding, workers, work, reservations, worker, at,
    );
}

pub(crate) proof fn actual_work_insertion_preserves(
    binding: &crate::SchedulerBinding,
    workers: Seq<crate::WorkerRecord>,
    work: Seq<crate::WorkRecord>,
    reservations: Seq<crate::SchedulerReservation>,
    record: crate::WorkRecord,
    at: int,
)
    requires
        reservations::reservation_invariant_parts(binding, workers, work, reservations),
        0 <= at <= work.len(),
        reservations::work_identity_absent(work, record.spec_definition().spec_id()),
    ensures reservations::reservation_invariant_parts(
        binding,
        workers,
        work.insert(at, record),
        reservations,
    ),
{
    reservations::work_insertion_preserves(
        binding, workers, work, reservations, record, at,
    );
}

pub(crate) proof fn actual_reservation_insertion_preserves(
    binding: &crate::SchedulerBinding,
    workers: Seq<crate::WorkerRecord>,
    work: Seq<crate::WorkRecord>,
    reservations: Seq<crate::SchedulerReservation>,
    reservation: crate::SchedulerReservation,
    at: int,
)
    requires
        reservations::reservation_invariant_parts(binding, workers, work, reservations),
        reservations::reservation_feasible_parts(
            binding,
            workers,
            work,
            reservations,
            &reservation,
        ),
        0 <= at <= reservations.len(),
    ensures
        reservations::reservation_invariant_parts(
            binding,
            workers,
            work,
            reservations.insert(at, reservation),
        ),
{
    reservations::insertion_preserves(
        binding,
        workers,
        work,
        reservations,
        reservation,
        at,
    );
}

pub(crate) proof fn actual_reservation_removal_preserves(
    binding: &crate::SchedulerBinding,
    workers: Seq<crate::WorkerRecord>,
    work: Seq<crate::WorkRecord>,
    reservations: Seq<crate::SchedulerReservation>,
    at: int,
)
    requires
        reservations::reservation_invariant_parts(binding, workers, work, reservations),
        0 <= at < reservations.len(),
    ensures
        reservations::reservation_invariant_parts(
            binding,
            workers,
            work,
            reservations.remove(at),
        ),
{
    reservations::removal_preserves(binding, workers, work, reservations, at);
}

pub(crate) proof fn actual_reservation_work_update_preserves(
    binding: &crate::SchedulerBinding,
    workers: Seq<crate::WorkerRecord>,
    before: Seq<crate::WorkRecord>,
    after: Seq<crate::WorkRecord>,
    reservations: Seq<crate::SchedulerReservation>,
    at: int,
)
    requires
        reservations::reservation_invariant_parts(binding, workers, before, reservations),
        before.len() == after.len(),
        0 <= at < before.len(),
        crate::WorkRecord::reservation_binding_equivalent(&before[at], &after[at]),
        forall |index: int| #![auto]
            0 <= index < before.len() && index != at ==> before[index] == after[index],
    ensures
        reservations::reservation_invariant_parts(binding, workers, after, reservations),
{
    reservations::work_update_preserves(
        binding,
        workers,
        before,
        after,
        reservations,
        at,
    );
}

pub(crate) proof fn actual_unreserved_work_update_preserves(
    binding: &crate::SchedulerBinding,
    workers: Seq<crate::WorkerRecord>,
    before: Seq<crate::WorkRecord>,
    after: Seq<crate::WorkRecord>,
    reservations: Seq<crate::SchedulerReservation>,
    at: int,
)
    requires
        reservations::reservation_invariant_parts(binding, workers, before, reservations),
        before.len() == after.len(),
        0 <= at < before.len(),
        crate::WorkRecord::reservation_subject_equivalent(&before[at], &after[at]),
        forall |index: int| #![auto]
            0 <= index < before.len() && index != at ==> before[index] == after[index],
        forall |reservation_index: int| #![auto]
            0 <= reservation_index < reservations.len() ==>
                reservations[reservation_index].spec_work_id()
                    != before[at].spec_definition().spec_id(),
    ensures reservations::reservation_invariant_parts(
        binding,
        workers,
        after,
        reservations,
    ),
{
    reservations::unreserved_work_update_preserves(
        binding,
        workers,
        before,
        after,
        reservations,
        at,
    );
}

pub(crate) proof fn actual_reservation_worker_update_preserves(
    binding: &crate::SchedulerBinding,
    before: Seq<crate::WorkerRecord>,
    after: Seq<crate::WorkerRecord>,
    work: Seq<crate::WorkRecord>,
    reservations: Seq<crate::SchedulerReservation>,
    at: int,
)
    requires
        reservations::reservation_invariant_parts(binding, before, work, reservations),
        before.len() == after.len(),
        0 <= at < before.len(),
        crate::WorkerRecord::reservation_owner_equivalent(&before[at], &after[at]),
        forall |index: int| #![auto]
            0 <= index < before.len() && index != at ==> before[index] == after[index],
    ensures
        reservations::reservation_invariant_parts(binding, after, work, reservations),
{
    reservations::worker_update_preserves(
        binding,
        before,
        after,
        work,
        reservations,
        at,
    );
}

} // verus!
