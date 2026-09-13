//! Preservation proof for reservation-stable worker lifecycle updates.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::{ResourceKind, SchedulerBinding, SchedulerReservation, WorkRecord, WorkerRecord};

#[cfg(verus_only)]
use super::{
    reservation_invariant_parts, reservations_have_workers, vector_quantity, worker_count,
    worker_identities_unique, worker_quantity,
};

verus! {

/// Replacing one worker record while retaining its reservation owner fields preserves OBL-0149.
pub(super) proof fn stable_worker_update_preserves(
    binding: &SchedulerBinding,
    before: Seq<WorkerRecord>,
    after: Seq<WorkerRecord>,
    work: Seq<WorkRecord>,
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
    WorkerRecord::reservation_owner_fields(&before[at], &after[at]);
    reveal(reservation_invariant_parts);
    reveal(worker_identities_unique);
    reveal(reservations_have_workers);
    assert(worker_identities_unique(after)) by {
        assert forall |left: int, right: int| #![auto]
            0 <= left < after.len()
                && 0 <= right < after.len()
                && left != right
            implies after[left].spec_descriptor().spec_id()
                != after[right].spec_descriptor().spec_id() by {
            if left == at {
                assert(before[right] == after[right]);
            } else if right == at {
                assert(before[left] == after[left]);
            } else {
                assert(before[left] == after[left]);
                assert(before[right] == after[right]);
            }
        }
    }
    assert(reservations_have_workers(after, reservations)) by {
        assert forall |reservation_index: int| #![trigger reservations[reservation_index]]
            0 <= reservation_index < reservations.len()
            implies exists |worker_index: int|
                #![trigger after[worker_index].spec_descriptor().spec_id()]
                0 <= worker_index < after.len()
                    && after[worker_index].spec_descriptor().spec_id()
                        == reservations[reservation_index].spec_worker_id()
                    && crate::identity::actor_ids_match(
                        after[worker_index].spec_descriptor().spec_owner(),
                        reservations[reservation_index].spec_owner(),
                    ) by {
            let old_index = choose |worker_index: int|
                #![trigger before[worker_index].spec_descriptor().spec_id()]
                0 <= worker_index < before.len()
                    && before[worker_index].spec_descriptor().spec_id()
                        == reservations[reservation_index].spec_worker_id()
                    && crate::identity::actor_ids_match(
                        before[worker_index].spec_descriptor().spec_owner(),
                        reservations[reservation_index].spec_owner(),
                    );
            if old_index == at {
                assert(WorkerRecord::reservation_owner_equivalent(&before[at], &after[at]));
            } else {
                assert(before[old_index] == after[old_index]);
            }
        }
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
            assert(WorkerRecord::reservation_owner_equivalent(&before[at], &after[at]));
        } else {
            assert(before[worker_index] == after[worker_index]);
        }
    }
}

} // verus!
