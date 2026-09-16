//! Root cancellation composed with completion and acknowledgement command contracts.

use super::command::{CancellationRejection, command_matches, root_exists, root_lifecycle};
use crate::state::mutation::{
    AcknowledgeCancellationOutcome, CompleteCommandOutcome,
    acknowledge_cancellation_outcome_matches, cancelling_dispatch_cannot_resurrect,
    complete_command_outcome_matches, work_update_preserves_other_state,
};
use crate::{
    DispatchId, SchedulerCommandKind, SchedulerEventKind, SchedulerState, WorkId, WorkPhase,
    WorkRecord, WorkTerminal,
};
use vstd::prelude::*;

verus! {

pub open spec fn live_root_dispatch(
    state: &SchedulerState,
    root: WorkId,
    dispatch_id: DispatchId,
) -> bool {
    exists |reservation_index: int, work_index: int| {
        &&& 0 <= reservation_index < state.spec_reservations().len()
        &&& state.spec_reservations()[reservation_index].spec_dispatch_id() == dispatch_id
        &&& state.spec_reservations()[reservation_index].spec_work_id() == root
        &&& 0 <= work_index < state.spec_work().len()
        &&& state.spec_work()[work_index].spec_definition().spec_id() == root
        &&& state.spec_work()[work_index].spec_phase() != WorkPhase::Terminal
    }
}

/// Final relationship for the production cancellation, completion, and acknowledgement chain.
pub open spec fn root_nonresurrection_matches(
    before: &SchedulerState,
    cancelled: &SchedulerState,
    after_late_completion: &SchedulerState,
    acknowledged: &SchedulerState,
    final_state: &SchedulerState,
    root: WorkId,
    dispatch_id: DispatchId,
    descendants: bool,
    cancellation_result: &Result<SchedulerEventKind, CancellationRejection>,
    late_outcome: CompleteCommandOutcome,
    acknowledgement_outcome: AcknowledgeCancellationOutcome,
    later_outcome: CompleteCommandOutcome,
) -> bool {
    &&& match cancellation_result {
        Ok(SchedulerEventKind::WorkCancelled {
            work_id, descendants: emitted_descendants, affected,
        }) => {
            *work_id == root && *emitted_descendants == descendants && affected@.contains(root)
        },
        _ => false,
    }
    &&& before.spec_reservation_reducer_ready()
            && crate::state::queue::queue_bound(before)
        ==> crate::state::queue::queue_bound(cancelled)
    &&& late_outcome == CompleteCommandOutcome::NotAcknowledgedRunning
    &&& *after_late_completion == *cancelled
    &&& acknowledgement_outcome == AcknowledgeCancellationOutcome::Applied
    &&& later_outcome == CompleteCommandOutcome::DispatchNotActive
    &&& *final_state == *acknowledged
    &&& forall |index: int| #![trigger acknowledged.spec_reservations()[index]]
        0 <= index < acknowledged.spec_reservations().len() ==>
            acknowledged.spec_reservations()[index].spec_dispatch_id() != dispatch_id
    &&& exists |index: int| #![trigger acknowledged.spec_work()[index]] {
        &&& 0 <= index < acknowledged.spec_work().len()
        &&& acknowledged.spec_work()[index].spec_definition().spec_id() == root
        &&& acknowledged.spec_work()[index].spec_phase() == WorkPhase::Terminal
        &&& acknowledged.spec_work()[index].spec_terminal() == Some(WorkTerminal::Cancelled)
    }
}

/// A successful actual root cancellation establishes the premise for the exact command chain.
pub(super) proof fn root_cancellation_cannot_resurrect(
    before: &SchedulerState,
    cancelled: &SchedulerState,
    after_late_completion: &SchedulerState,
    acknowledged: &SchedulerState,
    final_state: &SchedulerState,
    root: WorkId,
    dispatch_id: DispatchId,
    descendants: bool,
    cancellation_result: &Result<SchedulerEventKind, CancellationRejection>,
    late_completion: &SchedulerCommandKind,
    late_outcome: CompleteCommandOutcome,
    acknowledgement: &SchedulerCommandKind,
    acknowledgement_outcome: AcknowledgeCancellationOutcome,
    later_completion: &SchedulerCommandKind,
    later_outcome: CompleteCommandOutcome,
)
    requires
        before.spec_reservation_reducer_ready(),
        before.spec_collections_ordered(),
        live_root_dispatch(before, root, dispatch_id),
        command_matches(before, cancelled, root, descendants, cancellation_result),
        match late_completion {
            SchedulerCommandKind::CompleteWork { dispatch_id: requested, .. } => {
                *requested == dispatch_id
            },
            _ => false,
        },
        complete_command_outcome_matches(
            cancelled, after_late_completion, late_completion, late_outcome,
        ),
        match acknowledgement {
            SchedulerCommandKind::AcknowledgeCancellation { dispatch_id: requested } => {
                *requested == dispatch_id
            },
            _ => false,
        },
        acknowledge_cancellation_outcome_matches(
            after_late_completion, acknowledged, acknowledgement, acknowledgement_outcome,
        ),
        match later_completion {
            SchedulerCommandKind::CompleteWork { dispatch_id: requested, .. } => {
                *requested == dispatch_id
            },
            _ => false,
        },
        complete_command_outcome_matches(
            acknowledged, final_state, later_completion, later_outcome,
        ),
    ensures root_nonresurrection_matches(
        before,
        cancelled,
        after_late_completion,
        acknowledged,
        final_state,
        root,
        dispatch_id,
        descendants,
        cancellation_result,
        late_outcome,
        acknowledgement_outcome,
        later_outcome,
    ),
{
    reveal(live_root_dispatch);
    let reservation_index = choose |reservation_index: int|
        #![trigger before.spec_reservations()[reservation_index]]
        exists |work_index: int| #![trigger before.spec_work()[work_index]] {
        &&& 0 <= reservation_index < before.spec_reservations().len()
        &&& before.spec_reservations()[reservation_index].spec_dispatch_id() == dispatch_id
        &&& before.spec_reservations()[reservation_index].spec_work_id() == root
        &&& 0 <= work_index < before.spec_work().len()
        &&& before.spec_work()[work_index].spec_definition().spec_id() == root
        &&& before.spec_work()[work_index].spec_phase() != WorkPhase::Terminal
    };
    let work_index = choose |work_index: int| #![trigger before.spec_work()[work_index]] {
        &&& 0 <= work_index < before.spec_work().len()
        &&& before.spec_work()[work_index].spec_definition().spec_id() == root
        &&& before.spec_work()[work_index].spec_phase() != WorkPhase::Terminal
    };
    reveal(command_matches);
    reveal(SchedulerState::spec_collections_ordered);
    reveal(SchedulerState::spec_work_ordered);
    match cancellation_result {
        Err(CancellationRejection::WorkNotRetained) => {
            reveal(root_exists);
            assert(false);
        },
        Err(CancellationRejection::WorkAlreadyTerminal) => {
            reveal(root_lifecycle);
            let terminal_index = choose |index: int| #![trigger before.spec_work()[index]] {
                &&& 0 <= index < before.spec_work().len()
                &&& before.spec_work()[index].spec_definition().spec_id() == root
                &&& before.spec_work()[index].spec_phase() == WorkPhase::Terminal
            };
            reveal(SchedulerState::spec_reservation_reducer_ready);
            reveal(crate::verified::reservation_invariant_parts);
            reveal(crate::verified::work_identities_unique);
            assert(terminal_index == work_index);
            assert(false);
        },
        Ok(SchedulerEventKind::WorkCancelled { affected, .. }) => {
            let predicate = |record: WorkRecord|
                super::selected(before.spec_work(), root, descendants, record);
            assert(predicate(before.spec_work()[work_index]));
            before.spec_work().lemma_filter_contains(predicate, work_index);
            let filtered = before.spec_work().filter(predicate);
            let filtered_index = choose |index: int| #![trigger filtered[index]]
                0 <= index < filtered.len() && filtered[index] == before.spec_work()[work_index];
            assert(affected@ == filtered.map_values(|record: WorkRecord|
                record.spec_definition().spec_id()));
            assert(affected@[filtered_index] == root);
            assert(affected@.contains(root));
            reveal(super::update::cancellation_final_matches);
            reveal(super::update::has_active_reservation);
            assert(super::update::has_active_reservation(before.spec_reservations(), root));
            super::update::active_target_finishes_cancelling(
                before.spec_work(),
                cancelled.spec_work(),
                before.spec_reservations(),
                affected@,
                root,
            );
            let cancelled_work = choose |index: int| #![trigger cancelled.spec_work()[index]] {
                &&& 0 <= index < cancelled.spec_work().len()
                &&& cancelled.spec_work()[index].spec_definition().spec_id() == root
                &&& cancelled.spec_work()[index].spec_phase() == WorkPhase::Cancelling
            };
            reveal(work_update_preserves_other_state);
            assert(exists |target_reservation: int, target_work: int| {
                &&& 0 <= target_reservation < cancelled.spec_reservations().len()
                &&& cancelled.spec_reservations()[target_reservation].spec_dispatch_id()
                    == dispatch_id
                &&& cancelled.spec_reservations()[target_reservation].spec_work_id() == root
                &&& 0 <= target_work < cancelled.spec_work().len()
                &&& cancelled.spec_work()[target_work].spec_definition().spec_id() == root
                &&& cancelled.spec_work()[target_work].spec_phase() == WorkPhase::Cancelling
            }) by {
                assert(cancelled.spec_reservations()[reservation_index]
                    == before.spec_reservations()[reservation_index]);
                assert(cancelled.spec_work()[cancelled_work].spec_phase()
                    == WorkPhase::Cancelling);
            }
            cancelling_dispatch_cannot_resurrect(
                cancelled,
                after_late_completion,
                acknowledged,
                final_state,
                dispatch_id,
                root,
                late_completion,
                late_outcome,
                acknowledgement,
                acknowledgement_outcome,
                later_completion,
                later_outcome,
            );
            reveal(root_nonresurrection_matches);
        },
        Ok(_) => { assert(false); },
    }
}

} // verus!
