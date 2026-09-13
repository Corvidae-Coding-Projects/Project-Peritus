//! Queue-bound preservation for one classified worker-loss release.

use peritus_types::Sha256Digest;
use vstd::prelude::*;

use crate::state::{mutation, queue};
use crate::{
    DispatchId, LossOutcome, SchedulerReservation, SchedulerSemantics, SchedulerState, WorkPhase,
    WorkRecord,
};

use super::{loss_release_matches, outcome_matches, release_effect_matches};

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

proof fn classified_record_was_active(
    state: &SchedulerState,
    dispatch_id: DispatchId,
    outcome: LossOutcome,
    work_index: int,
    removed: &SchedulerReservation,
    after_reservations: Seq<SchedulerReservation>,
)
    requires
        state.spec_reservation_reducer_ready(),
        0 <= work_index < state.spec_work().len(),
        outcome_matches(state.spec_work()[work_index], dispatch_id, outcome),
        removed.spec_work_id()
            == state.spec_work()[work_index].spec_definition().spec_id(),
        mutation::exact_reservation_removal_matches(
            state.spec_reservations(),
            after_reservations,
            dispatch_id,
            Some(*removed),
        ),
    ensures queue::is_active(state.spec_work()[work_index]),
{
    reveal(mutation::exact_reservation_removal_matches);
    let reservation_index = choose |index: int| #![trigger state.spec_reservations()[index]] {
        &&& 0 <= index < state.spec_reservations().len()
        &&& state.spec_reservations()[index].spec_dispatch_id() == dispatch_id
        &&& *removed == state.spec_reservations()[index]
    };
    reveal(outcome_matches);
    reveal(SchedulerState::spec_reservation_reducer_ready);
    reveal(crate::verified::reservations_bind_active_work);
    reveal(crate::verified::reservation_has_active_work);
    let matching_work = choose |index: int|
        #![trigger state.spec_work()[index].spec_definition().spec_id()] {
        &&& 0 <= index < state.spec_work().len()
        &&& state.spec_work()[index].spec_definition().spec_id()
            == state.spec_reservations()[reservation_index].spec_work_id()
        &&& crate::verified::work_phase_retains_reservation(
            state.spec_work()[index].spec_phase(),
        )
    };
    reveal(crate::verified::reservation_invariant_parts);
    reveal(crate::verified::work_identities_unique);
    assert(state.spec_reservations()[reservation_index].spec_work_id()
        == state.spec_work()[work_index].spec_definition().spec_id());
    assert(matching_work == work_index);
    reveal(crate::verified::work_phase_retains_reservation);
    reveal(queue::is_active);
}

proof fn release_predicates_do_not_increase(
    before: &SchedulerState,
    after: &SchedulerState,
    dispatch_id: DispatchId,
    failure_digest: Sha256Digest,
    outcome: LossOutcome,
)
    requires
        before.spec_reservation_reducer_ready(),
        loss_release_matches(before, after, dispatch_id, failure_digest, outcome),
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
    reveal(loss_release_matches);
    let work_index = choose |work_index: int| #![trigger before.spec_work()[work_index]]
        exists |removed: SchedulerReservation|
            #![trigger release_effect_matches(
                before, after, dispatch_id, failure_digest, outcome, &removed,
            )] {
        &&& 0 <= work_index < before.spec_work().len()
        &&& outcome_matches(before.spec_work()[work_index], dispatch_id, outcome)
        &&& release_effect_matches(
            before, after, dispatch_id, failure_digest, outcome, &removed,
        )
    };
    let removed = choose |removed: SchedulerReservation|
        #![trigger release_effect_matches(
            before, after, dispatch_id, failure_digest, outcome, &removed,
        )] {
        &&& 0 <= work_index < before.spec_work().len()
        &&& outcome_matches(before.spec_work()[work_index], dispatch_id, outcome)
        &&& release_effect_matches(
            before, after, dispatch_id, failure_digest, outcome, &removed,
        )
    };
    reveal(release_effect_matches);
    match outcome {
        LossOutcome::Requeued { work_id, .. } => {
            reveal(mutation::phase_release_matches);
            classified_record_was_active(
                before,
                dispatch_id,
                outcome,
                work_index,
                &removed,
                after.spec_reservations(),
            );
            reveal(mutation::work_phase_update_matches);
            let changed = choose |index: int| #![trigger before.spec_work()[index]] {
                &&& 0 <= index < before.spec_work().len()
                &&& mutation::work_record_update_matches(
                    before.spec_work()[index], after.spec_work()[index], work_id,
                    WorkPhase::Queued,
                )
                &&& forall |other: int| #![auto]
                    0 <= other < before.spec_work().len() && other != index ==>
                        after.spec_work()[other] == before.spec_work()[other]
            };
            reveal(mutation::work_record_update_matches);
            reveal(outcome_matches);
            reveal(SchedulerState::spec_reservation_reducer_ready);
            reveal(crate::verified::reservation_invariant_parts);
            reveal(crate::verified::work_identities_unique);
            assert(changed == work_index);
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
        },
        LossOutcome::Cancelled { work_id, .. }
        | LossOutcome::Exhausted { work_id, .. }
        | LossOutcome::Ambiguous { work_id, .. }
        | LossOutcome::Failed { work_id, .. } => {
            reveal(mutation::terminal_release_matches);
            reveal(mutation::work_terminal_update_matches);
            let changed = choose |index: int| #![trigger before.spec_work()[index]] {
                &&& 0 <= index < before.spec_work().len()
                &&& mutation::work_record_update_matches(
                    before.spec_work()[index], after.spec_work()[index], work_id,
                    WorkPhase::Terminal,
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
        },
    }
}

/// One exact classified loss release preserves either persisted queue bound.
pub(super) proof fn loss_release_preserves_queue_bound(
    before: &SchedulerState,
    after: &SchedulerState,
    dispatch_id: DispatchId,
    failure_digest: Sha256Digest,
    outcome: LossOutcome,
)
    requires
        before.spec_reservation_reducer_ready(),
        queue::queue_bound(before),
        loss_release_matches(before, after, dispatch_id, failure_digest, outcome),
    ensures queue::queue_bound(after),
{
    release_predicates_do_not_increase(before, after, dispatch_id, failure_digest, outcome);
    let strict = |record: WorkRecord| queue::is_waiting(record) || queue::can_return(record);
    let legacy = |record: WorkRecord| queue::is_waiting(record) || queue::is_active(record);
    filtered_count_nonincreasing(before.spec_work(), after.spec_work(), strict);
    filtered_count_nonincreasing(before.spec_work(), after.spec_work(), legacy);
    reveal(queue::queue_bound);
    reveal(queue::admission_pressure);
    match before.spec_binding().spec_semantics() {
        SchedulerSemantics::LegacyQueueV1 => {},
        SchedulerSemantics::StrictRecoveryQueueV2 => {},
    }
}

} // verus!
