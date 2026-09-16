//! Preservation proof for reservation-stable work lifecycle updates.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::{SchedulerBinding, SchedulerReservation, WorkRecord, WorkerRecord};

#[cfg(verus_only)]
use super::{reservation_invariant_parts, reservations_have_work, work_identities_unique};

verus! {

/// Replacing one work record while retaining its reservation binding preserves OBL-0149.
pub(super) proof fn stable_work_update_preserves(
    binding: &SchedulerBinding,
    workers: Seq<WorkerRecord>,
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    at: int,
)
    requires
        reservation_invariant_parts(binding, workers, before, reservations),
        before.len() == after.len(),
        0 <= at < before.len(),
        WorkRecord::reservation_binding_equivalent(&before[at], &after[at]),
        forall |index: int| #![auto]
            0 <= index < before.len() && index != at ==> before[index] == after[index],
    ensures
        reservation_invariant_parts(binding, workers, after, reservations),
{
    WorkRecord::reservation_binding_fields(&before[at], &after[at]);
    reveal(reservation_invariant_parts);
    reveal(work_identities_unique);
    reveal(reservations_have_work);
    assert(work_identities_unique(after)) by {
        assert forall |left: int, right: int| #![auto]
            0 <= left < after.len()
                && 0 <= right < after.len()
                && left != right
            implies after[left].spec_definition().spec_id()
                != after[right].spec_definition().spec_id() by {
            if left == at {
                assert(before[at].spec_definition().spec_id()
                    == after[at].spec_definition().spec_id());
                assert(before[right] == after[right]);
            } else if right == at {
                assert(before[at].spec_definition().spec_id()
                    == after[at].spec_definition().spec_id());
                assert(before[left] == after[left]);
            } else {
                assert(before[left] == after[left]);
                assert(before[right] == after[right]);
            }
        }
    }
    assert(reservations_have_work(after, reservations)) by {
        assert forall |reservation_index: int| #![trigger reservations[reservation_index]]
            0 <= reservation_index < reservations.len()
            implies exists |work_index: int|
                #![trigger after[work_index].spec_definition().spec_id()]
                0 <= work_index < after.len()
                    && after[work_index].spec_definition().spec_id()
                        == reservations[reservation_index].spec_work_id()
                    && crate::identity::actor_ids_match(
                        after[work_index].spec_definition().spec_owner(),
                        reservations[reservation_index].spec_owner(),
                    )
                    && after[work_index].spec_definition().spec_revision()
                        == reservations[reservation_index].spec_revision()
                    && after[work_index].spec_definition().spec_request().spec_entries()
                        == reservations[reservation_index].spec_resources().spec_entries()
                    && after[work_index].spec_attempts_started()
                        == reservations[reservation_index].spec_attempt().spec_value() by {
            let old_index = choose |work_index: int|
                #![trigger before[work_index].spec_definition().spec_id()]
                0 <= work_index < before.len()
                    && before[work_index].spec_definition().spec_id()
                        == reservations[reservation_index].spec_work_id()
                    && crate::identity::actor_ids_match(
                        before[work_index].spec_definition().spec_owner(),
                        reservations[reservation_index].spec_owner(),
                    )
                    && before[work_index].spec_definition().spec_revision()
                        == reservations[reservation_index].spec_revision()
                    && before[work_index].spec_definition().spec_request().spec_entries()
                        == reservations[reservation_index].spec_resources().spec_entries()
                    && before[work_index].spec_attempts_started()
                        == reservations[reservation_index].spec_attempt().spec_value();
            if old_index == at {
                assert(WorkRecord::reservation_binding_equivalent(&before[at], &after[at]));
            } else {
                assert(before[old_index] == after[old_index]);
            }
        }
    }
}

/// Replacing one unreserved work record may advance its attempt without affecting OBL-0149.
pub(super) proof fn unreserved_work_update_preserves(
    binding: &SchedulerBinding,
    workers: Seq<WorkerRecord>,
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    reservations: Seq<SchedulerReservation>,
    at: int,
)
    requires
        reservation_invariant_parts(binding, workers, before, reservations),
        before.len() == after.len(),
        0 <= at < before.len(),
        WorkRecord::reservation_subject_equivalent(&before[at], &after[at]),
        forall |index: int| #![auto]
            0 <= index < before.len() && index != at ==> before[index] == after[index],
        forall |reservation_index: int| #![auto]
            0 <= reservation_index < reservations.len() ==>
                reservations[reservation_index].spec_work_id()
                    != before[at].spec_definition().spec_id(),
    ensures
        reservation_invariant_parts(binding, workers, after, reservations),
{
    WorkRecord::reservation_subject_fields(&before[at], &after[at]);
    reveal(reservation_invariant_parts);
    reveal(work_identities_unique);
    reveal(reservations_have_work);
    assert(work_identities_unique(after)) by {
        assert forall |left: int, right: int| #![auto]
            0 <= left < after.len()
                && 0 <= right < after.len()
                && left != right
            implies after[left].spec_definition().spec_id()
                != after[right].spec_definition().spec_id() by {
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
    assert(reservations_have_work(after, reservations)) by {
        assert forall |reservation_index: int| #![trigger reservations[reservation_index]]
            0 <= reservation_index < reservations.len()
            implies exists |work_index: int|
                #![trigger after[work_index].spec_definition().spec_id()]
                0 <= work_index < after.len()
                    && after[work_index].spec_definition().spec_id()
                        == reservations[reservation_index].spec_work_id()
                    && crate::identity::actor_ids_match(
                        after[work_index].spec_definition().spec_owner(),
                        reservations[reservation_index].spec_owner(),
                    )
                    && after[work_index].spec_definition().spec_revision()
                        == reservations[reservation_index].spec_revision()
                    && after[work_index].spec_definition().spec_request().spec_entries()
                        == reservations[reservation_index].spec_resources().spec_entries()
                    && after[work_index].spec_attempts_started()
                        == reservations[reservation_index].spec_attempt().spec_value() by {
            let old_index = choose |work_index: int|
                #![trigger before[work_index].spec_definition().spec_id()]
                0 <= work_index < before.len()
                    && before[work_index].spec_definition().spec_id()
                        == reservations[reservation_index].spec_work_id()
                    && crate::identity::actor_ids_match(
                        before[work_index].spec_definition().spec_owner(),
                        reservations[reservation_index].spec_owner(),
                    )
                    && before[work_index].spec_definition().spec_revision()
                        == reservations[reservation_index].spec_revision()
                    && before[work_index].spec_definition().spec_request().spec_entries()
                        == reservations[reservation_index].spec_resources().spec_entries()
                    && before[work_index].spec_attempts_started()
                        == reservations[reservation_index].spec_attempt().spec_value();
            if old_index == at {
                assert(reservations[reservation_index].spec_work_id()
                    == before[at].spec_definition().spec_id());
                assert(false);
            }
            assert(before[old_index] == after[old_index]);
            assert(exists |work_index: int|
                #![trigger after[work_index].spec_definition().spec_id()]
                0 <= work_index < after.len()
                    && after[work_index].spec_definition().spec_id()
                        == reservations[reservation_index].spec_work_id()
                    && crate::identity::actor_ids_match(
                        after[work_index].spec_definition().spec_owner(),
                        reservations[reservation_index].spec_owner(),
                    )
                    && after[work_index].spec_definition().spec_revision()
                        == reservations[reservation_index].spec_revision()
                    && after[work_index].spec_definition().spec_request().spec_entries()
                        == reservations[reservation_index].spec_resources().spec_entries()
                    && after[work_index].spec_attempts_started()
                        == reservations[reservation_index].spec_attempt().spec_value()) by {
                assert(0 <= old_index < after.len());
            }
        }
    }
}

} // verus!
