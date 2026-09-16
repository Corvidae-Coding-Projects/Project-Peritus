//! Preservation proof for releasing one exact reservation.

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
    reservation_quantity_nonnegative, reservation_quantity_remove, vector_quantity_nonnegative,
    worker_count_nonnegative, worker_count_remove, worker_quantity_nonnegative,
    worker_quantity_remove,
};

verus! {

/// Removing one exact reservation preserves every capacity upper bound and unrelated owner.
pub(super) proof fn exact_removal_preserves(
    binding: &SchedulerBinding,
    workers: Seq<WorkerRecord>,
    work: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    at: int,
)
    requires
        reservation_invariant_parts(binding, workers, work, reservations),
        0 <= at < reservations.len(),
    ensures
        reservation_invariant_parts(binding, workers, work, reservations.remove(at)),
{
    reservations.remove_ensures(at);
    reveal(reservation_invariant_parts);
    reveal(reservations_have_workers);
    reveal(reservations_have_work);
    assert(reservations_have_workers(workers, reservations));
    assert(reservations_have_work(work, reservations));
    assert(reservation_identities_unique(reservations.remove(at))) by {
        assert forall |left: int, right: int| #![auto]
            0 <= left < reservations.remove(at).len()
                && 0 <= right < reservations.remove(at).len()
                && left != right
            implies reservations.remove(at)[left].spec_dispatch_id()
                    != reservations.remove(at)[right].spec_dispatch_id()
                && reservations.remove(at)[left].spec_work_id()
                    != reservations.remove(at)[right].spec_work_id() by {
            let old_left = if left < at { left } else { left + 1 };
            let old_right = if right < at { right } else { right + 1 };
            assert(0 <= old_left < reservations.len());
            assert(0 <= old_right < reservations.len());
            assert(old_left != old_right);
            assert(reservations.remove(at)[left] == reservations[old_left]);
            assert(reservations.remove(at)[right] == reservations[old_right]);
        }
    }
    assert(reservations_have_workers(workers, reservations.remove(at))) by {
        assert forall |reservation_index: int|
            #![trigger reservations.remove(at)[reservation_index].spec_worker_id()]
            0 <= reservation_index < reservations.remove(at).len()
            implies exists |worker_index: int|
                #![trigger workers[worker_index].spec_descriptor().spec_id()]
                0 <= worker_index < workers.len()
                    && workers[worker_index].spec_descriptor().spec_id()
                        == reservations.remove(at)[reservation_index].spec_worker_id()
                    && crate::identity::actor_ids_match(
                        workers[worker_index].spec_descriptor().spec_owner(),
                        reservations.remove(at)[reservation_index].spec_owner(),
                    ) by {
            let old_index = if reservation_index < at {
                reservation_index
            } else {
                reservation_index + 1
            };
            assert(0 <= old_index < reservations.len());
            assert(reservations.remove(at)[reservation_index] == reservations[old_index]);
            assert(exists |worker_index: int|
                #![trigger workers[worker_index].spec_descriptor().spec_id()]
                0 <= worker_index < workers.len()
                    && workers[worker_index].spec_descriptor().spec_id()
                        == reservations[old_index].spec_worker_id()
                    && crate::identity::actor_ids_match(
                        workers[worker_index].spec_descriptor().spec_owner(),
                        reservations[old_index].spec_owner(),
                    ));
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
                        == reservations.remove(at)[reservation_index].spec_worker_id()
                    && crate::identity::actor_ids_match(
                        workers[candidate].spec_descriptor().spec_owner(),
                        reservations.remove(at)[reservation_index].spec_owner(),
                    )) by {
                assert(0 <= worker_index < workers.len());
            }
        }
    }
    assert(reservations_have_work(work, reservations.remove(at))) by {
        assert forall |reservation_index: int|
            #![trigger reservations.remove(at)[reservation_index].spec_work_id()]
            0 <= reservation_index < reservations.remove(at).len()
            implies exists |work_index: int|
                #![trigger work[work_index].spec_definition().spec_id()]
                0 <= work_index < work.len()
                    && work[work_index].spec_definition().spec_id()
                        == reservations.remove(at)[reservation_index].spec_work_id()
                    && crate::identity::actor_ids_match(
                        work[work_index].spec_definition().spec_owner(),
                        reservations.remove(at)[reservation_index].spec_owner(),
                    )
                    && work[work_index].spec_definition().spec_revision()
                        == reservations.remove(at)[reservation_index].spec_revision()
                    && work[work_index].spec_definition().spec_request().spec_entries()
                        == reservations.remove(at)[reservation_index]
                            .spec_resources().spec_entries()
                    && work[work_index].spec_attempts_started()
                        == reservations.remove(at)[reservation_index].spec_attempt().spec_value()
                    by {
            let old_index = if reservation_index < at {
                reservation_index
            } else {
                reservation_index + 1
            };
            assert(0 <= old_index < reservations.len());
            assert(reservations.remove(at)[reservation_index] == reservations[old_index]);
            assert(exists |work_index: int|
                #![trigger work[work_index].spec_definition().spec_id()]
                0 <= work_index < work.len()
                    && work[work_index].spec_definition().spec_id()
                        == reservations[old_index].spec_work_id()
                    && crate::identity::actor_ids_match(
                        work[work_index].spec_definition().spec_owner(),
                        reservations[old_index].spec_owner(),
                    )
                    && work[work_index].spec_definition().spec_revision()
                        == reservations[old_index].spec_revision()
                    && work[work_index].spec_definition().spec_request().spec_entries()
                        == reservations[old_index].spec_resources().spec_entries()
                    && work[work_index].spec_attempts_started()
                        == reservations[old_index].spec_attempt().spec_value()
                    );
        }
    }
    assert forall |kind: ResourceKind|
        #![trigger reservation_quantity(reservations.remove(at), kind)]
        0 <= reservation_quantity(reservations.remove(at), kind)
            <= vector_quantity(binding.spec_capacity().spec_entries(), kind) by {
        reservation_quantity_remove(reservations, at, kind);
        reservation_quantity_nonnegative(reservations.remove(at), kind);
        vector_quantity_nonnegative(reservations[at].spec_resources().spec_entries(), kind);
    }
    assert forall |worker_index: int|
        #![trigger workers[worker_index].spec_descriptor().spec_id()]
        0 <= worker_index < workers.len() implies {
            let descriptor = workers[worker_index].spec_descriptor();
            &&& 0 <= worker_count(reservations.remove(at), descriptor.spec_id())
                <= descriptor.spec_concurrency()
            &&& forall |kind: ResourceKind|
                #![trigger worker_quantity(
                    reservations.remove(at), descriptor.spec_id(), kind)]
                0 <= worker_quantity(reservations.remove(at), descriptor.spec_id(), kind)
                    <= vector_quantity(descriptor.spec_capacity().spec_entries(), kind)
        } by {
        let worker_id = workers[worker_index].spec_descriptor().spec_id();
        worker_count_remove(reservations, at, worker_id);
        worker_count_nonnegative(reservations.remove(at), worker_id);
        assert forall |kind: ResourceKind|
            #![trigger worker_quantity(reservations.remove(at), worker_id, kind)]
            0 <= worker_quantity(reservations.remove(at), worker_id, kind)
                <= vector_quantity(
                    workers[worker_index].spec_descriptor().spec_capacity().spec_entries(),
                    kind,
                ) by {
            worker_quantity_remove(reservations, at, worker_id, kind);
            worker_quantity_nonnegative(reservations.remove(at), worker_id, kind);
            vector_quantity_nonnegative(reservations[at].spec_resources().spec_entries(), kind);
        }
    }
}

} // verus!
