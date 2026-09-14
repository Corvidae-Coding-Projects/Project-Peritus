//! Queue-bound preservation for one exact dependency refresh change.

use vstd::prelude::*;

use crate::state::{mutation, queue};
use crate::{SchedulerSemantics, SchedulerState, WorkPhase, WorkRecord, WorkTerminal};

use super::{DependencyAction, DependencyChange, change_applicable, dependency_step_matches};

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

proof fn applicable_and_updated_indices_match(
    before: &SchedulerState,
    change: DependencyChange,
    applicable: int,
    updated: int,
)
    requires
        before.spec_reservation_reducer_ready(),
        0 <= applicable < before.spec_work().len(),
        0 <= updated < before.spec_work().len(),
        super::change_target_matches(before.spec_work()[applicable], change),
        before.spec_work()[updated].spec_definition().spec_id()
            == change.spec_work_id(),
    ensures applicable == updated,
{
    reveal(super::change_target_matches);
    reveal(SchedulerState::spec_reservation_reducer_ready);
    reveal(crate::verified::reservation_invariant_parts);
    reveal(crate::verified::work_identities_unique);
}

proof fn ready_step_predicates_do_not_increase(
    before: &SchedulerState,
    after: &SchedulerState,
    change: DependencyChange,
)
    requires
        before.spec_reservation_reducer_ready(),
        dependency_step_matches(before, after, change),
        change.spec_action() == DependencyAction::Ready,
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
    reveal(dependency_step_matches);
    reveal(change_applicable);
    let applicable = choose |index: int| #![trigger before.spec_work()[index]] {
        &&& 0 <= index < before.spec_work().len()
        &&& super::change_target_matches(before.spec_work()[index], change)
    };
    reveal(mutation::work_phase_update_matches);
    let updated = choose |index: int| #![trigger before.spec_work()[index]] {
        &&& 0 <= index < before.spec_work().len()
        &&& mutation::work_record_update_matches(
            before.spec_work()[index], after.spec_work()[index],
            change.spec_work_id(), WorkPhase::Queued,
        )
        &&& forall |other: int| #![auto]
            0 <= other < before.spec_work().len() && other != index ==>
                after.spec_work()[other] == before.spec_work()[other]
    };
    reveal(mutation::work_record_update_matches);
    applicable_and_updated_indices_match(before, change, applicable, updated);
    reveal(super::change_target_matches);
    WorkRecord::lifecycle_update_fields(
        &before.spec_work()[updated],
        &after.spec_work()[updated],
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
        if index != updated {
            assert(after.spec_work()[index] == before.spec_work()[index]);
        }
    }
    assert forall |index: int| #![trigger after.spec_work()[index]]
        0 <= index < after.spec_work().len()
            && (queue::is_waiting(after.spec_work()[index])
                || queue::is_active(after.spec_work()[index]))
        implies queue::is_waiting(before.spec_work()[index])
            || queue::is_active(before.spec_work()[index]) by {
        if index != updated {
            assert(after.spec_work()[index] == before.spec_work()[index]);
        }
    }
}

proof fn failed_step_predicates_do_not_increase(
    before: &SchedulerState,
    after: &SchedulerState,
    change: DependencyChange,
    dependency: crate::WorkId,
)
    requires
        before.spec_reservation_reducer_ready(),
        dependency_step_matches(before, after, change),
        change.spec_action() == (DependencyAction::Failed { dependency }),
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
    reveal(dependency_step_matches);
    reveal(change_applicable);
    let applicable = choose |index: int| #![trigger before.spec_work()[index]] {
        &&& 0 <= index < before.spec_work().len()
        &&& super::change_target_matches(before.spec_work()[index], change)
    };
    reveal(mutation::work_terminal_update_matches);
    let updated = choose |index: int| #![trigger before.spec_work()[index]] {
        &&& 0 <= index < before.spec_work().len()
        &&& mutation::work_record_update_matches(
            before.spec_work()[index], after.spec_work()[index],
            change.spec_work_id(), WorkPhase::Terminal,
        )
        &&& after.spec_work()[index].spec_terminal()
            == Some(WorkTerminal::DependencyFailed { dependency })
        &&& forall |other: int| #![auto]
            0 <= other < before.spec_work().len() && other != index ==>
                after.spec_work()[other] == before.spec_work()[other]
    };
    reveal(mutation::work_record_update_matches);
    applicable_and_updated_indices_match(before, change, applicable, updated);
    reveal(queue::is_waiting);
    reveal(queue::can_return);
    reveal(queue::is_active);
    assert forall |index: int| #![trigger after.spec_work()[index]]
        0 <= index < after.spec_work().len()
            && (queue::is_waiting(after.spec_work()[index])
                || queue::can_return(after.spec_work()[index]))
        implies queue::is_waiting(before.spec_work()[index])
            || queue::can_return(before.spec_work()[index]) by {
        if index != updated {
            assert(after.spec_work()[index] == before.spec_work()[index]);
        }
    }
    assert forall |index: int| #![trigger after.spec_work()[index]]
        0 <= index < after.spec_work().len()
            && (queue::is_waiting(after.spec_work()[index])
                || queue::is_active(after.spec_work()[index]))
        implies queue::is_waiting(before.spec_work()[index])
            || queue::is_active(before.spec_work()[index]) by {
        if index != updated {
            assert(after.spec_work()[index] == before.spec_work()[index]);
        }
    }
}

proof fn step_predicates_do_not_increase(
    before: &SchedulerState,
    after: &SchedulerState,
    change: DependencyChange,
)
    requires
        before.spec_reservation_reducer_ready(),
        dependency_step_matches(before, after, change),
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
    match change.spec_action() {
        DependencyAction::Ready => {
            ready_step_predicates_do_not_increase(before, after, change);
        },
        DependencyAction::Failed { dependency } => {
            failed_step_predicates_do_not_increase(before, after, change, dependency);
        },
        DependencyAction::NoChange => {
            reveal(dependency_step_matches);
            assert(false);
        },
    }
}

/// A Ready step preserves one waiting slot; a Failed step removes one waiting slot.
pub(super) proof fn dependency_step_preserves_queue_bound(
    before: &SchedulerState,
    after: &SchedulerState,
    change: DependencyChange,
)
    requires
        before.spec_reservation_reducer_ready(),
        queue::queue_bound(before),
        dependency_step_matches(before, after, change),
    ensures queue::queue_bound(after),
{
    step_predicates_do_not_increase(before, after, change);
    let strict = |record: WorkRecord| queue::is_waiting(record) || queue::can_return(record);
    let legacy = |record: WorkRecord| queue::is_waiting(record) || queue::is_active(record);
    filtered_count_nonincreasing(before.spec_work(), after.spec_work(), strict);
    filtered_count_nonincreasing(before.spec_work(), after.spec_work(), legacy);
    reveal(dependency_step_matches);
    reveal(mutation::work_update_preserves_other_state);
    reveal(queue::queue_bound);
    reveal(queue::admission_pressure);
    match before.spec_binding().spec_semantics() {
        SchedulerSemantics::LegacyQueueV1 => {},
        SchedulerSemantics::StrictRecoveryQueueV2 => {},
    }
}

} // verus!
