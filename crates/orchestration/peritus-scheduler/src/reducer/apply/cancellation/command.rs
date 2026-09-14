//! Exact root admission and event construction for production cancellation.

#[cfg(verus_only)]
use crate::WorkRecord;
use crate::{SchedulerEventKind, SchedulerState, WorkId, WorkPhase};
use vstd::prelude::*;

verus! {

/// Rejections emitted before any cancellation mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationRejection {
    /// The requested work identity is not retained.
    WorkNotRetained,
    /// The requested work is already terminal.
    WorkAlreadyTerminal,
}

/// Whether the requested identity occurs in the pre-state.
pub open spec fn root_exists(state: &SchedulerState, root: WorkId) -> bool {
    exists |index: int| #![trigger state.spec_work()[index]]
        0 <= index < state.spec_work().len()
            && state.spec_work()[index].spec_definition().spec_id() == root
}

/// A retained root with the supplied terminal classification.
pub open spec fn root_lifecycle(
    state: &SchedulerState,
    root: WorkId,
    terminal: bool,
) -> bool {
    exists |index: int| #![trigger state.spec_work()[index]]
        0 <= index < state.spec_work().len()
            && state.spec_work()[index].spec_definition().spec_id() == root
            && (state.spec_work()[index].spec_phase() == WorkPhase::Terminal) == terminal
}

/// Exact cancellation result, selected event payload and successor work relation.
pub open spec fn command_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    root: WorkId,
    descendants: bool,
    result: &Result<SchedulerEventKind, CancellationRejection>,
) -> bool {
    match result {
        Err(CancellationRejection::WorkNotRetained) => {
            &&& *after == *before
            &&& before.spec_work_ordered() ==> !root_exists(before, root)
        },
        Err(CancellationRejection::WorkAlreadyTerminal) => {
            &&& *after == *before
            &&& root_lifecycle(before, root, true)
        },
        Ok(SchedulerEventKind::WorkCancelled {
            work_id, descendants: emitted_descendants, affected,
        }) => {
            &&& *work_id == root
            &&& *emitted_descendants == descendants
            &&& root_lifecycle(before, root, false)
            &&& affected@ == before.spec_work().filter(|record: WorkRecord|
                super::selected(before.spec_work(), root, descendants, record))
                    .map_values(|record: WorkRecord| record.spec_definition().spec_id())
            &&& super::update::cancellation_final_matches(
                before.spec_work(), after.spec_work(), before.spec_reservations(), affected@,
            )
            &&& crate::state::mutation::work_update_preserves_other_state(before, after)
            &&& before.spec_reservation_invariant() ==> after.spec_reservation_invariant()
            &&& before.spec_reservation_reducer_ready() ==> after.spec_reservation_reducer_ready()
            &&& before.spec_collections_ordered() ==> after.spec_collections_ordered()
            &&& before.spec_collections_ordered() ==> affected@.no_duplicates()
            &&& before.spec_reservation_reducer_ready()
                    && crate::state::queue::queue_bound(before)
                ==> crate::state::queue::queue_bound(after)
        },
        Ok(_) => false,
    }
}

/// Runs root admission, the actual cancellation update and exact event construction.
///
/// # Errors
/// Rejects an absent or already-terminal root before changing state.
pub fn apply_command(
    state: &mut SchedulerState,
    root: WorkId,
    descendants: bool,
) -> (result: Result<SchedulerEventKind, CancellationRejection>)
    ensures command_matches(old(state), final(state), root, descendants, &result),
{
    let Some(retained) = state.work_item(root) else {
        proof { reveal(root_exists); }
        return Err(CancellationRejection::WorkNotRetained);
    };
    let terminal = retained.phase().same(WorkPhase::Terminal);
    proof {
        assert(root_lifecycle(state, root, terminal));
    }
    if terminal {
        return Err(CancellationRejection::WorkAlreadyTerminal);
    }
    let (affected, _) = super::cancel_retained(state, root, descendants);
    proof {
        if old(state).spec_collections_ordered() {
            reveal(SchedulerState::spec_collections_ordered);
            reveal(SchedulerState::spec_work_ordered);
        }
    }
    Ok(SchedulerEventKind::WorkCancelled { work_id: root, descendants, affected })
}

} // verus!
