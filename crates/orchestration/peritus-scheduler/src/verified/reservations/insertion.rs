//! Preservation proof for adding one selected feasible reservation.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::{ResourceKind, SchedulerBinding, SchedulerReservation, WorkRecord, WorkerRecord};

#[cfg(verus_only)]
use super::{
    reservation_identities_unique, reservation_invariant_parts, reservation_quantity,
    reservations_have_work, reservations_have_workers, vector_quantity, worker_count,
    worker_quantity,
};

#[cfg(verus_only)]
use super::arithmetic::{
    reservation_quantity_insert, reservation_quantity_nonnegative, vector_quantity_nonnegative,
    worker_count_insert, worker_count_nonnegative, worker_quantity_insert,
    worker_quantity_nonnegative,
};

verus! {

/// Inserting one selected feasible reservation preserves global and worker capacities.
pub(super) proof fn feasible_insertion_preserves(
    binding: &SchedulerBinding,
    workers: Seq<WorkerRecord>,
    work: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    reservation: SchedulerReservation,
    at: int,
)
    requires
        reservation_invariant_parts(binding, workers, work, reservations),
        0 <= at <= reservations.len(),
        reservations.len() < binding.spec_limits().spec_active_reservations(),
        forall |index: int| #![auto] 0 <= index < reservations.len() ==>
            reservations[index].spec_dispatch_id() != reservation.spec_dispatch_id()
                && reservations[index].spec_work_id() != reservation.spec_work_id(),
        exists |worker_index: int| #![auto]
            0 <= worker_index < workers.len()
                && workers[worker_index].spec_descriptor().spec_id()
                    == reservation.spec_worker_id()
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
                        ),
        forall |kind: ResourceKind|
            #![trigger reservation_quantity(reservations, kind)]
            reservation_quantity(reservations, kind)
                    + vector_quantity(reservation.spec_resources().spec_entries(), kind)
                <= vector_quantity(binding.spec_capacity().spec_entries(), kind),
        exists |work_index: int| #![auto]
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
                    == reservation.spec_attempt().spec_value(),
    ensures
        reservation_invariant_parts(
            binding,
            workers,
            work,
            reservations.insert(at, reservation),
        ),
{
    reservations.insert_ensures(at, reservation);
    let selected_worker = choose |worker_index: int| #![auto]
        0 <= worker_index < workers.len()
            && workers[worker_index].spec_descriptor().spec_id()
                == reservation.spec_worker_id()
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
                    );
    assert(reservation_identities_unique(reservations.insert(at, reservation))) by {
        assert forall |left: int, right: int| #![auto]
            0 <= left < reservations.insert(at, reservation).len()
                && 0 <= right < reservations.insert(at, reservation).len()
                && left != right
            implies reservations.insert(at, reservation)[left].spec_dispatch_id()
                    != reservations.insert(at, reservation)[right].spec_dispatch_id()
                && reservations.insert(at, reservation)[left].spec_work_id()
                    != reservations.insert(at, reservation)[right].spec_work_id() by {
            if left == at || right == at {
                let old_index = if left == at {
                    if right < at { right } else { right - 1 }
                } else if left < at {
                    left
                } else {
                    left - 1
                };
                assert(0 <= old_index < reservations.len());
                assert(reservations.insert(at, reservation)[at] == reservation);
                if left == at {
                    assert(reservations.insert(at, reservation)[right]
                        == reservations[old_index]);
                } else {
                    assert(reservations.insert(at, reservation)[left]
                        == reservations[old_index]);
                }
            } else {
                let old_left = if left < at { left } else { left - 1 };
                let old_right = if right < at { right } else { right - 1 };
                assert(0 <= old_left < reservations.len());
                assert(0 <= old_right < reservations.len());
                assert(old_left != old_right);
                assert(reservations.insert(at, reservation)[left] == reservations[old_left]);
                assert(reservations.insert(at, reservation)[right] == reservations[old_right]);
            }
        }
    }
    assert(reservations_have_workers(workers, reservations.insert(at, reservation))) by {
        assert forall |reservation_index: int|
            #![trigger reservations.insert(at, reservation)[reservation_index].spec_worker_id()]
            0 <= reservation_index < reservations.insert(at, reservation).len()
            implies exists |worker_index: int|
                #![trigger workers[worker_index].spec_descriptor().spec_id()]
                0 <= worker_index < workers.len()
                    && workers[worker_index].spec_descriptor().spec_id()
                        == reservations.insert(at, reservation)[reservation_index].spec_worker_id()
                    && crate::identity::actor_ids_match(
                        workers[worker_index].spec_descriptor().spec_owner(),
                        reservations.insert(at, reservation)[reservation_index].spec_owner(),
                    ) by {
            if reservation_index == at {
                assert(reservations.insert(at, reservation)[reservation_index] == reservation);
                assert(exists |worker_index: int|
                    #![trigger workers[worker_index].spec_descriptor().spec_id()]
                    0 <= worker_index < workers.len()
                        && workers[worker_index].spec_descriptor().spec_id()
                            == reservation.spec_worker_id()
                        && crate::identity::actor_ids_match(
                            workers[worker_index].spec_descriptor().spec_owner(),
                            reservation.spec_owner(),
                        ));
            } else {
                let old_index = if reservation_index < at {
                    reservation_index
                } else {
                    reservation_index - 1
                };
                assert(0 <= old_index < reservations.len());
                assert(reservations.insert(at, reservation)[reservation_index]
                    == reservations[old_index]);
                let worker_index = choose |worker_index: int|
                    #![trigger workers[worker_index].spec_descriptor().spec_id()]
                    0 <= worker_index < workers.len()
                        && workers[worker_index].spec_descriptor().spec_id()
                            == reservations[old_index].spec_worker_id()
                        && crate::identity::actor_ids_match(
                            workers[worker_index].spec_descriptor().spec_owner(),
                            reservations[old_index].spec_owner(),
                        );
                assert(exists |candidate: int|
                    #![trigger workers[candidate].spec_descriptor().spec_id()]
                    0 <= candidate < workers.len()
                        && workers[candidate].spec_descriptor().spec_id()
                            == reservations.insert(at, reservation)[reservation_index]
                                .spec_worker_id()
                        && crate::identity::actor_ids_match(
                            workers[candidate].spec_descriptor().spec_owner(),
                            reservations.insert(at, reservation)[reservation_index].spec_owner(),
                        ))
                    by {
                    assert(0 <= worker_index < workers.len());
                }
            }
        }
    }
    assert(reservations_have_work(work, reservations.insert(at, reservation))) by {
        assert forall |reservation_index: int|
            #![trigger reservations.insert(at, reservation)[reservation_index].spec_work_id()]
            0 <= reservation_index < reservations.insert(at, reservation).len()
            implies exists |work_index: int|
                #![trigger work[work_index].spec_definition().spec_id()]
                0 <= work_index < work.len()
                    && work[work_index].spec_definition().spec_id()
                        == reservations.insert(at, reservation)[reservation_index].spec_work_id()
                    && crate::identity::actor_ids_match(
                        work[work_index].spec_definition().spec_owner(),
                        reservations.insert(at, reservation)[reservation_index].spec_owner(),
                    )
                    && work[work_index].spec_definition().spec_revision()
                        == reservations.insert(at, reservation)[reservation_index].spec_revision()
                    && work[work_index].spec_definition().spec_request().spec_entries()
                        == reservations.insert(at, reservation)[reservation_index]
                            .spec_resources().spec_entries()
                    && work[work_index].spec_attempts_started()
                        == reservations.insert(at, reservation)[reservation_index]
                            .spec_attempt().spec_value()
                    by {
            if reservation_index == at {
                assert(reservations.insert(at, reservation)[reservation_index] == reservation);
            } else {
                let old_index = if reservation_index < at {
                    reservation_index
                } else {
                    reservation_index - 1
                };
                assert(0 <= old_index < reservations.len());
                assert(reservations.insert(at, reservation)[reservation_index]
                    == reservations[old_index]);
            }
        }
    }
    assert forall |kind: ResourceKind|
        #![trigger reservation_quantity(reservations.insert(at, reservation), kind)]
        0 <= reservation_quantity(reservations.insert(at, reservation), kind)
            <= vector_quantity(binding.spec_capacity().spec_entries(), kind) by {
        reservation_quantity_insert(reservations, reservation, at, kind);
        reservation_quantity_nonnegative(reservations, kind);
        vector_quantity_nonnegative(reservation.spec_resources().spec_entries(), kind);
    }
    assert forall |worker_index: int|
        #![trigger workers[worker_index].spec_descriptor().spec_id()]
        0 <= worker_index < workers.len() implies {
            let descriptor = workers[worker_index].spec_descriptor();
            &&& 0 <= worker_count(
                reservations.insert(at, reservation),
                descriptor.spec_id(),
            ) <= descriptor.spec_concurrency()
            &&& forall |kind: ResourceKind|
                #![trigger worker_quantity(
                    reservations.insert(at, reservation),
                    descriptor.spec_id(),
                    kind,
                )]
                0 <= worker_quantity(
                    reservations.insert(at, reservation),
                    descriptor.spec_id(),
                    kind,
                ) <= vector_quantity(descriptor.spec_capacity().spec_entries(), kind)
        } by {
        let worker_id = workers[worker_index].spec_descriptor().spec_id();
        worker_count_insert(reservations, reservation, at, worker_id);
        worker_count_nonnegative(reservations, worker_id);
        if worker_id == reservation.spec_worker_id() && worker_index != selected_worker {
            assert(false);
        }
        assert forall |kind: ResourceKind|
            #![trigger worker_quantity(
                reservations.insert(at, reservation),
                worker_id,
                kind,
            )]
            0 <= worker_quantity(
                reservations.insert(at, reservation),
                worker_id,
                kind,
            ) <= vector_quantity(
                workers[worker_index].spec_descriptor().spec_capacity().spec_entries(),
                kind,
            ) by {
            worker_quantity_insert(reservations, reservation, at, worker_id, kind);
            worker_quantity_nonnegative(reservations, worker_id, kind);
            vector_quantity_nonnegative(reservation.spec_resources().spec_entries(), kind);
        }
    }
}

} // verus!
