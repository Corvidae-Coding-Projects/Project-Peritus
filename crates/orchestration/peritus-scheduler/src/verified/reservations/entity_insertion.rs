//! Preservation when production admission retains a new worker or work item.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::{ResourceKind, SchedulerBinding, SchedulerReservation, WorkRecord, WorkerRecord};

#[cfg(verus_only)]
use super::{
    reservation_has_work, reservation_invariant_parts, reservations_have_work,
    reservations_have_work_intro, reservations_have_workers, vector_quantity,
    work_binds_reservation, work_identities_unique, work_identity_absent, worker_count,
    worker_identities_unique, worker_identity_absent, worker_quantity,
};

#[cfg(verus_only)]
use super::arithmetic::{
    vector_quantity_nonnegative, worker_count_nonnegative, worker_quantity_nonnegative,
};

verus! {

proof fn absent_worker_has_zero_count(
    reservations: Seq<SchedulerReservation>,
    worker: crate::WorkerId,
)
    requires forall |index: int| #![auto]
        0 <= index < reservations.len() ==>
            reservations[index].spec_worker_id() != worker,
    ensures worker_count(reservations, worker) == 0,
    decreases reservations.len(),
{
    if reservations.len() > 0 {
        assert(reservations.first().spec_worker_id() != worker);
        assert forall |index: int| #![auto]
            0 <= index < reservations.drop_first().len() implies
                reservations.drop_first()[index].spec_worker_id() != worker by {
            assert(reservations.drop_first()[index] == reservations[index + 1]);
        }
        absent_worker_has_zero_count(reservations.drop_first(), worker);
    }
}

proof fn absent_worker_has_zero_quantity(
    reservations: Seq<SchedulerReservation>,
    worker: crate::WorkerId,
    kind: ResourceKind,
)
    requires forall |index: int| #![auto]
        0 <= index < reservations.len() ==>
            reservations[index].spec_worker_id() != worker,
    ensures worker_quantity(reservations, worker, kind) == 0,
    decreases reservations.len(),
{
    if reservations.len() > 0 {
        assert(reservations.first().spec_worker_id() != worker);
        assert forall |index: int| #![auto]
            0 <= index < reservations.drop_first().len() implies
                reservations.drop_first()[index].spec_worker_id() != worker by {
            assert(reservations.drop_first()[index] == reservations[index + 1]);
        }
        absent_worker_has_zero_quantity(reservations.drop_first(), worker, kind);
    }
}

pub(super) proof fn worker_insertion_preserves(
    binding: &SchedulerBinding,
    workers: Seq<WorkerRecord>,
    work: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    worker: WorkerRecord,
    at: int,
)
    requires
        reservation_invariant_parts(binding, workers, work, reservations),
        0 <= at <= workers.len(),
        worker_identity_absent(workers, worker.spec_descriptor().spec_id()),
    ensures reservation_invariant_parts(
        binding, workers.insert(at, worker), work, reservations,
    ),
{
    reveal(worker_identity_absent);
    reveal(reservation_invariant_parts);
    reveal(reservations_have_workers);
    reveal(worker_identities_unique);
    workers.insert_ensures(at, worker);
    let after = workers.insert(at, worker);
    assert(worker_identities_unique(after)) by {
        assert forall |left: int, right: int| #![auto]
            0 <= left < after.len() && 0 <= right < after.len() && left != right
            implies after[left].spec_descriptor().spec_id()
                != after[right].spec_descriptor().spec_id() by {
            if left == at || right == at {
                let old_index = if left == at {
                    if right < at { right } else { right - 1 }
                } else if left < at { left } else { left - 1 };
                assert(0 <= old_index < workers.len());
                assert(after[at] == worker);
                if left == at {
                    assert(after[right] == workers[old_index]);
                } else {
                    assert(after[left] == workers[old_index]);
                }
            } else {
                let old_left = if left < at { left } else { left - 1 };
                let old_right = if right < at { right } else { right - 1 };
                assert(0 <= old_left < workers.len());
                assert(0 <= old_right < workers.len());
                assert(old_left != old_right);
                assert(after[left] == workers[old_left]);
                assert(after[right] == workers[old_right]);
            }
        }
    }
    assert(reservations_have_workers(after, reservations)) by {
        assert forall |reservation_index: int| #![trigger reservations[reservation_index]]
            0 <= reservation_index < reservations.len() implies
                exists |worker_index: int|
                    #![trigger after[worker_index].spec_descriptor().spec_id()]
                    0 <= worker_index < after.len()
                        && after[worker_index].spec_descriptor().spec_id()
                            == reservations[reservation_index].spec_worker_id()
                        && crate::identity::actor_ids_match(
                            after[worker_index].spec_descriptor().spec_owner(),
                            reservations[reservation_index].spec_owner(),
                        ) by {
            let old_index = choose |candidate: int|
                #![trigger workers[candidate].spec_descriptor().spec_id()]
                0 <= candidate < workers.len()
                    && workers[candidate].spec_descriptor().spec_id()
                        == reservations[reservation_index].spec_worker_id()
                    && crate::identity::actor_ids_match(
                        workers[candidate].spec_descriptor().spec_owner(),
                        reservations[reservation_index].spec_owner(),
                    );
            let new_index = if old_index < at { old_index } else { old_index + 1 };
            assert(0 <= new_index < after.len());
            assert(after[new_index] == workers[old_index]);
        }
    }
    assert forall |reservation_index: int| #![auto]
        0 <= reservation_index < reservations.len() implies
            reservations[reservation_index].spec_worker_id()
                != worker.spec_descriptor().spec_id() by {
        let old_index = choose |candidate: int|
            #![trigger workers[candidate].spec_descriptor().spec_id()]
            0 <= candidate < workers.len()
                && workers[candidate].spec_descriptor().spec_id()
                    == reservations[reservation_index].spec_worker_id()
                && crate::identity::actor_ids_match(
                    workers[candidate].spec_descriptor().spec_owner(),
                    reservations[reservation_index].spec_owner(),
                );
        assert(workers[old_index].spec_descriptor().spec_id()
            != worker.spec_descriptor().spec_id());
    }
    assert forall |worker_index: int|
        #![trigger after[worker_index].spec_descriptor().spec_id()]
        0 <= worker_index < after.len() implies {
            let descriptor = after[worker_index].spec_descriptor();
            &&& 0 <= worker_count(reservations, descriptor.spec_id())
                <= descriptor.spec_concurrency()
            &&& forall |kind: ResourceKind|
                #![trigger worker_quantity(reservations, descriptor.spec_id(), kind)]
                0 <= worker_quantity(reservations, descriptor.spec_id(), kind)
                    <= vector_quantity(descriptor.spec_capacity().spec_entries(), kind)
        } by {
        if worker_index == at {
            absent_worker_has_zero_count(reservations, worker.spec_descriptor().spec_id());
            assert forall |kind: ResourceKind|
                #![trigger worker_quantity(
                    reservations, worker.spec_descriptor().spec_id(), kind,
                )]
                0 <= worker_quantity(
                    reservations, worker.spec_descriptor().spec_id(), kind,
                ) <= vector_quantity(
                    worker.spec_descriptor().spec_capacity().spec_entries(), kind,
                ) by {
                absent_worker_has_zero_quantity(
                    reservations, worker.spec_descriptor().spec_id(), kind,
                );
                vector_quantity_nonnegative(
                    worker.spec_descriptor().spec_capacity().spec_entries(), kind,
                );
            }
        } else {
            let old_index = if worker_index < at { worker_index } else { worker_index - 1 };
            assert(0 <= old_index < workers.len());
            assert(after[worker_index] == workers[old_index]);
            worker_count_nonnegative(reservations, after[worker_index].spec_descriptor().spec_id());
            assert forall |kind: ResourceKind|
                #![trigger worker_quantity(
                    reservations, after[worker_index].spec_descriptor().spec_id(), kind,
                )]
                0 <= worker_quantity(
                    reservations, after[worker_index].spec_descriptor().spec_id(), kind,
                ) <= vector_quantity(
                    after[worker_index].spec_descriptor().spec_capacity().spec_entries(), kind,
                ) by {
                worker_quantity_nonnegative(
                    reservations, after[worker_index].spec_descriptor().spec_id(), kind,
                );
            }
        }
    }
}

pub(super) proof fn work_insertion_preserves(
    binding: &SchedulerBinding,
    workers: Seq<WorkerRecord>,
    work: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    record: WorkRecord,
    at: int,
)
    requires
        reservation_invariant_parts(binding, workers, work, reservations),
        0 <= at <= work.len(),
        work_identity_absent(work, record.spec_definition().spec_id()),
    ensures reservation_invariant_parts(
        binding, workers, work.insert(at, record), reservations,
    ),
{
    reveal(work_identity_absent);
    reveal(reservation_invariant_parts);
    reveal(reservations_have_work);
    reveal(work_identities_unique);
    work.insert_ensures(at, record);
    let after = work.insert(at, record);
    assert(work_identities_unique(after)) by {
        assert forall |left: int, right: int| #![auto]
            0 <= left < after.len() && 0 <= right < after.len() && left != right
            implies after[left].spec_definition().spec_id()
                != after[right].spec_definition().spec_id() by {
            if left == at || right == at {
                let old_index = if left == at {
                    if right < at { right } else { right - 1 }
                } else if left < at { left } else { left - 1 };
                assert(0 <= old_index < work.len());
                assert(after[at] == record);
                if left == at {
                    assert(after[right] == work[old_index]);
                } else {
                    assert(after[left] == work[old_index]);
                }
            } else {
                let old_left = if left < at { left } else { left - 1 };
                let old_right = if right < at { right } else { right - 1 };
                assert(0 <= old_left < work.len());
                assert(0 <= old_right < work.len());
                assert(old_left != old_right);
                assert(after[left] == work[old_left]);
                assert(after[right] == work[old_right]);
            }
        }
    }
    assert forall |reservation_index: int| #![trigger reservations[reservation_index]]
            0 <= reservation_index < reservations.len() implies
                reservation_has_work(after, &reservations[reservation_index]) by {
            let reservation = reservations[reservation_index];
            assert(reservation == reservations[reservation_index]);
            reveal(reservation_has_work);
            let old_index = choose |candidate: int|
                #![trigger work[candidate].spec_definition().spec_id()]
                0 <= candidate < work.len()
                    && work_binds_reservation(
                        &work[candidate],
                        &reservations[reservation_index],
                    );
            let new_index = if old_index < at { old_index } else { old_index + 1 };
            assert(0 <= new_index < after.len());
            assert(after[new_index] == work[old_index]);
        }
    reservations_have_work_intro(after, reservations);
}

} // verus!
