//! Queue-bound preservation for reservation-sensitive cancellation updates.

use super::update::{cancellation_step_matches, has_active_reservation};
use crate::state::{mutation, queue};
use crate::{SchedulerSemantics, SchedulerState, WorkPhase, WorkRecord};
use vstd::prelude::*;

verus! {

proof fn filtered_count_nonincreasing<A>(
    before: Seq<A>,
    after: Seq<A>,
    predicate: spec_fn(A) -> bool,
)
    requires
        before.len() == after.len(),
        forall |index: int| #![trigger after[index]] 0 <= index < after.len()
            && predicate(after[index]) ==> predicate(before[index]),
    ensures after.filter(predicate).len() <= before.filter(predicate).len(),
    decreases before.len(),
{
    if before.len() > 0 {
        let before_prefix = before.drop_last();
        let after_prefix = after.drop_last();
        assert forall |index: int| #![trigger after_prefix[index]]
            0 <= index < after_prefix.len() && predicate(after_prefix[index])
            implies predicate(before_prefix[index]) by {
            assert(after_prefix[index] == after[index]);
            assert(before_prefix[index] == before[index]);
        }
        filtered_count_nonincreasing(before_prefix, after_prefix, predicate);
        before_prefix.lemma_filter_len_push(predicate, before.last());
        after_prefix.lemma_filter_len_push(predicate, after.last());
        assert(before_prefix.push(before.last()) =~= before);
        assert(after_prefix.push(after.last()) =~= after);
        if predicate(after.last()) {
            assert(predicate(before.last()));
        }
    }
}

proof fn active_target_was_active(
    state: &SchedulerState,
    id: crate::WorkId,
    work_index: int,
)
    requires
        state.spec_reservation_reducer_ready(),
        has_active_reservation(state.spec_reservations(), id),
        0 <= work_index < state.spec_work().len(),
        state.spec_work()[work_index].spec_definition().spec_id() == id,
    ensures queue::is_active(state.spec_work()[work_index]),
{
    reveal(has_active_reservation);
    let reservation_index = choose |index: int| #![trigger state.spec_reservations()[index]]
        0 <= index < state.spec_reservations().len()
            && state.spec_reservations()[index].spec_work_id() == id;
    reveal(SchedulerState::spec_reservation_reducer_ready);
    reveal(crate::verified::reservations_bind_active_work);
    reveal(crate::verified::reservation_has_active_work);
    let matching_work = choose |index: int|
        #![trigger state.spec_work()[index].spec_definition().spec_id()]
        0 <= index < state.spec_work().len()
            && state.spec_work()[index].spec_definition().spec_id()
                == state.spec_reservations()[reservation_index].spec_work_id()
            && crate::verified::work_phase_retains_reservation(
                state.spec_work()[index].spec_phase(),
            );
    reveal(crate::verified::work_phase_retains_reservation);
    reveal(crate::verified::reservation_invariant_parts);
    reveal(crate::verified::work_identities_unique);
    assert(matching_work == work_index);
    reveal(queue::is_active);
}

proof fn step_predicates_do_not_increase(
    before: &SchedulerState,
    after: &SchedulerState,
    id: crate::WorkId,
)
    requires
        before.spec_reservation_reducer_ready(),
        cancellation_step_matches(
            before.spec_work(), after.spec_work(), before.spec_reservations(), id,
        ),
        mutation::work_update_preserves_other_state(before, after),
    ensures
        before.spec_work().len() == after.spec_work().len(),
        forall |index: int| #![trigger after.spec_work()[index]]
            0 <= index < after.spec_work().len()
                && (queue::is_waiting(after.spec_work()[index])
                    || queue::can_return(after.spec_work()[index])) ==>
                    queue::is_waiting(before.spec_work()[index])
                        || queue::can_return(before.spec_work()[index]),
        forall |index: int| #![trigger after.spec_work()[index]]
            0 <= index < after.spec_work().len()
                && (queue::is_waiting(after.spec_work()[index])
                    || queue::is_active(after.spec_work()[index])) ==>
                    queue::is_waiting(before.spec_work()[index])
                        || queue::is_active(before.spec_work()[index]),
{
    reveal(cancellation_step_matches);
    if has_active_reservation(before.spec_reservations(), id) {
        reveal(mutation::work_phase_update_matches);
        let changed = choose |index: int| #![trigger before.spec_work()[index]] {
            &&& 0 <= index < before.spec_work().len()
            &&& mutation::work_record_update_matches(
                before.spec_work()[index], after.spec_work()[index], id, WorkPhase::Cancelling,
            )
            &&& forall |other: int| #![auto]
                0 <= other < before.spec_work().len() && other != index ==>
                    after.spec_work()[other] == before.spec_work()[other]
        };
        reveal(mutation::work_record_update_matches);
        WorkRecord::lifecycle_update_fields(
            &before.spec_work()[changed], &after.spec_work()[changed],
        );
        active_target_was_active(before, id, changed);
        reveal(queue::is_waiting);
        reveal(queue::can_return);
        reveal(queue::is_active);
        assert forall |index: int| #![trigger after.spec_work()[index]]
            0 <= index < after.spec_work().len()
                && (queue::is_waiting(after.spec_work()[index])
                    || queue::can_return(after.spec_work()[index]))
            implies queue::is_waiting(before.spec_work()[index])
                || queue::can_return(before.spec_work()[index]) by {
            if index != changed {
                assert(after.spec_work()[index] == before.spec_work()[index]);
            }
        }
        assert forall |index: int| #![trigger after.spec_work()[index]]
            0 <= index < after.spec_work().len()
                && (queue::is_waiting(after.spec_work()[index])
                    || queue::is_active(after.spec_work()[index]))
            implies queue::is_waiting(before.spec_work()[index])
                || queue::is_active(before.spec_work()[index]) by {
            if index != changed {
                assert(after.spec_work()[index] == before.spec_work()[index]);
            }
        }
    } else {
        reveal(mutation::work_terminal_update_matches);
        let changed = choose |index: int| #![trigger before.spec_work()[index]] {
            &&& 0 <= index < before.spec_work().len()
            &&& mutation::work_record_update_matches(
                before.spec_work()[index], after.spec_work()[index], id, WorkPhase::Terminal,
            )
            &&& forall |other: int| #![auto]
                0 <= other < before.spec_work().len() && other != index ==>
                    after.spec_work()[other] == before.spec_work()[other]
        };
        reveal(mutation::work_record_update_matches);
        WorkRecord::lifecycle_update_fields(
            &before.spec_work()[changed], &after.spec_work()[changed],
        );
        reveal(queue::is_waiting);
        reveal(queue::can_return);
        reveal(queue::is_active);
        assert forall |index: int| #![trigger after.spec_work()[index]]
            0 <= index < after.spec_work().len()
                && (queue::is_waiting(after.spec_work()[index])
                    || queue::can_return(after.spec_work()[index]))
            implies queue::is_waiting(before.spec_work()[index])
                || queue::can_return(before.spec_work()[index]) by {
            if index != changed {
                assert(after.spec_work()[index] == before.spec_work()[index]);
            }
        }
        assert forall |index: int| #![trigger after.spec_work()[index]]
            0 <= index < after.spec_work().len()
                && (queue::is_waiting(after.spec_work()[index])
                    || queue::is_active(after.spec_work()[index]))
            implies queue::is_waiting(before.spec_work()[index])
                || queue::is_active(before.spec_work()[index]) by {
            if index != changed {
                assert(after.spec_work()[index] == before.spec_work()[index]);
            }
        }
    }
}

/// One exact cancellation update cannot increase either versioned queue measure.
pub(super) proof fn cancellation_step_preserves_queue_bound(
    before: &SchedulerState,
    after: &SchedulerState,
    id: crate::WorkId,
)
    requires
        before.spec_reservation_reducer_ready(),
        queue::queue_bound(before),
        cancellation_step_matches(
            before.spec_work(), after.spec_work(), before.spec_reservations(), id,
        ),
        mutation::work_update_preserves_other_state(before, after),
    ensures queue::queue_bound(after),
{
    step_predicates_do_not_increase(before, after, id);
    let strict = |record: WorkRecord| queue::is_waiting(record) || queue::can_return(record);
    let legacy = |record: WorkRecord| queue::is_waiting(record) || queue::is_active(record);
    filtered_count_nonincreasing(before.spec_work(), after.spec_work(), strict);
    filtered_count_nonincreasing(before.spec_work(), after.spec_work(), legacy);
    reveal(mutation::work_update_preserves_other_state);
    reveal(queue::queue_bound);
    reveal(queue::admission_pressure);
    match before.spec_binding().spec_semantics() {
        SchedulerSemantics::LegacyQueueV1 => {},
        SchedulerSemantics::StrictRecoveryQueueV2 => {},
    }
}

pub(super) proof fn unchanged_work_preserves_queue_bound(
    before: &SchedulerState,
    after: &SchedulerState,
)
    requires
        queue::queue_bound(before),
        before.spec_work() == after.spec_work(),
        mutation::work_update_preserves_other_state(before, after),
    ensures queue::queue_bound(after),
{
    reveal(mutation::work_update_preserves_other_state);
    reveal(queue::queue_bound);
    reveal(queue::admission_pressure);
}

} // verus!
