//! Finite application of one immutable dependency-refresh change set.

use vstd::prelude::*;

use crate::state::mutation;
use crate::{SchedulerState, WorkPhase, WorkTerminal};

#[cfg(verus_only)]
use super::super::dependency_measure::{dependency_measure, dependency_step_decreases_measure};
#[cfg(verus_only)]
use super::frame::{
    collection_order_preserved, compose_layout_and_frame, phase_update_preserves_ids,
    terminal_update_preserves_ids,
};
#[cfg(verus_only)]
use super::queue::dependency_step_preserves_queue_bound;
#[cfg(verus_only)]
use super::selection::{pending_change_is_applicable, selected_changes_are_unique};
#[cfg(verus_only)]
use super::trace::{advance_prefix, empty_updates};
use super::{DependencyAction, DependencyChange};
#[cfg(verus_only)]
use super::{
    change_applicable, change_ids, changes_match, dependency_step_matches,
    dependency_updates_match, unselected_work_unchanged, work_id_layout_matches,
};

verus! {

pub(super) open spec fn dependency_application_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    changes: Seq<DependencyChange>,
) -> bool {
    let valid = before.spec_reservation_reducer_ready()
        && before.spec_collections_ordered()
        && changes_match(before, before.spec_work(), changes);
    &&& valid ==> {
        &&& dependency_updates_match(before, after, changes)
        &&& unselected_work_unchanged(
            before.spec_work(), after.spec_work(), change_ids(changes),
        )
        &&& after.spec_reservation_reducer_ready()
        &&& after.spec_collections_ordered()
    }
    &&& valid && crate::state::queue::queue_bound(before) ==>
        crate::state::queue::queue_bound(after)
    &&& mutation::work_update_preserves_other_state(before, after)
    &&& work_id_layout_matches(before.spec_work(), after.spec_work())
    &&& valid && changes.len() > 0 ==>
        dependency_measure(after.spec_work()) < dependency_measure(before.spec_work())
    &&& changes.len() == 0 ==> *after == *before
}

proof fn applicable_change_is_admissible(
    state: &SchedulerState,
    change: DependencyChange,
)
    requires
        state.spec_reservation_reducer_ready(),
        change_applicable(state, change),
    ensures match change.spec_action() {
        DependencyAction::Ready => crate::verified::work_phase_update_admissible(
            state.spec_work(),
            state.spec_reservations(),
            change.spec_work_id(),
            WorkPhase::Queued,
        ),
        DependencyAction::Failed { .. } => crate::verified::work_phase_update_admissible(
            state.spec_work(),
            state.spec_reservations(),
            change.spec_work_id(),
            WorkPhase::Terminal,
        ),
        DependencyAction::NoChange => false,
    }
{
    reveal(change_applicable);
    let target = choose |index: int| #![trigger state.spec_work()[index]] {
        &&& 0 <= index < state.spec_work().len()
        &&& super::change_target_matches(state.spec_work()[index], change)
    };
    reveal(super::change_target_matches);
    reveal(crate::verified::work_phase_retains_reservation);
    match change.spec_action() {
        DependencyAction::Ready => {
            crate::verified::inactive_work_update_is_admissible(
                state, target, WorkPhase::Queued,
            );
        },
        DependencyAction::Failed { .. } => {
            crate::verified::inactive_work_update_is_admissible(
                state, target, WorkPhase::Terminal,
            );
        },
        DependencyAction::NoChange => assert(false),
    }
}

/// Applies one already-selected dependency change without adding caller preconditions.
fn apply_one(
    state: &mut SchedulerState,
    change: DependencyChange,
) -> (applied: bool)
    ensures
        mutation::work_update_preserves_other_state(old(state), final(state)),
        work_id_layout_matches(old(state).spec_work(), final(state).spec_work()),
        old(state).spec_collections_ordered() ==> final(state).spec_collections_ordered(),
        old(state).spec_reservation_reducer_ready()
                && change_applicable(old(state), change)
            ==> {
                &&& applied
                &&& dependency_step_matches(old(state), final(state), change)
                &&& final(state).spec_reservation_reducer_ready()
            },
        old(state).spec_reservation_reducer_ready()
                && change_applicable(old(state), change)
                && crate::state::queue::queue_bound(old(state))
            ==> crate::state::queue::queue_bound(final(state)),
{
    let ghost was_ready = state.spec_reservation_reducer_ready();
    let ghost was_ordered = state.spec_collections_ordered();
    let ghost had_queue_bound = crate::state::queue::queue_bound(state);
    let ghost applicable = change_applicable(state, change);
    proof {
        if was_ready && applicable {
            applicable_change_is_admissible(state, change);
        }
    }
    match change.action() {
        DependencyAction::Ready => {
            let applied = mutation::set_work_phase(
                state, change.work_id(), WorkPhase::Queued,
            );
            proof {
                phase_update_preserves_ids(
                    old(state).spec_work(),
                    state.spec_work(),
                    change.spec_work_id(),
                    WorkPhase::Queued,
                    applied,
                );
                collection_order_preserved(old(state), state);
                if was_ready && applicable {
                    reveal(dependency_step_matches);
                    assert(applied);
                    assert(dependency_step_matches(old(state), state, change));
                    if had_queue_bound {
                        dependency_step_preserves_queue_bound(old(state), state, change);
                    }
                }
            }
            applied
        },
        DependencyAction::Failed { dependency } => {
            let applied = mutation::terminalize_work(
                state,
                change.work_id(),
                WorkTerminal::DependencyFailed { dependency },
            );
            proof {
                terminal_update_preserves_ids(
                    old(state).spec_work(),
                    state.spec_work(),
                    change.spec_work_id(),
                    WorkTerminal::DependencyFailed { dependency },
                    applied,
                );
                collection_order_preserved(old(state), state);
                if was_ready && applicable {
                    reveal(dependency_step_matches);
                    assert(applied);
                    assert(dependency_step_matches(old(state), state, change));
                    if had_queue_bound {
                        dependency_step_preserves_queue_bound(old(state), state, change);
                    }
                }
            }
            applied
        },
        DependencyAction::NoChange => {
            proof {
                reveal(change_applicable);
                reveal(mutation::work_update_preserves_other_state);
                reveal(work_id_layout_matches);
                if was_ordered {
                    collection_order_preserved(old(state), state);
                }
            }
            false
        },
    }
}

/// Applies an exact immutable-snapshot change set in its collected order.
pub(super) fn apply_dependency_changes(
    state: &mut SchedulerState,
    changes: &[DependencyChange],
)
    ensures dependency_application_matches(old(state), final(state), changes@),
{
    let ghost initial = *state;
    let ghost valid = state.spec_reservation_reducer_ready()
        && state.spec_collections_ordered()
        && changes_match(state, state.spec_work(), changes@);
    let ghost had_queue_bound = crate::state::queue::queue_bound(state);
    let mut index = 0;
    proof {
        empty_updates(initial);
        reveal(mutation::work_update_preserves_other_state);
        if valid {
            selected_changes_are_unique(&initial, changes@);
        }
    }
    while index < changes.len()
        invariant
            index <= changes.len(),
            initial == *old(state),
            valid == (
                initial.spec_reservation_reducer_ready()
                    && initial.spec_collections_ordered()
                    && changes_match(&initial, initial.spec_work(), changes@)
            ),
            had_queue_bound == crate::state::queue::queue_bound(&initial),
            valid ==> change_ids(changes@).no_duplicates(),
            valid ==> state.spec_reservation_reducer_ready(),
            valid ==> state.spec_collections_ordered(),
            valid ==> dependency_measure(state.spec_work())
                <= dependency_measure(initial.spec_work()),
            valid && index > 0 ==> dependency_measure(state.spec_work())
                < dependency_measure(initial.spec_work()),
            valid && had_queue_bound ==> crate::state::queue::queue_bound(state),
            valid ==> dependency_updates_match(
                &initial, state, changes@.take(index as int),
            ),
            index == 0 ==> *state == initial,
            mutation::work_update_preserves_other_state(&initial, state),
            work_id_layout_matches(initial.spec_work(), state.spec_work()),
            valid ==> unselected_work_unchanged(
                initial.spec_work(),
                state.spec_work(),
                change_ids(changes@.take(index as int)),
            ),
        decreases changes.len() - index,
    {
        let change = changes[index];
        let ghost middle = *state;
        proof {
            if valid {
                pending_change_is_applicable(
                    &initial, state, changes@, index as int,
                );
            }
        }
        let _applied = apply_one(state, change);
        proof {
            if valid {
                assert(_applied);
                assert(dependency_step_matches(&middle, state, change));
                dependency_step_decreases_measure(&middle, state, change);
                assert(dependency_measure(state.spec_work())
                    < dependency_measure(initial.spec_work()));
                advance_prefix(
                    &initial,
                    &middle,
                    state,
                    changes@,
                    index as int,
                );
            } else {
                compose_layout_and_frame(&initial, &middle, state);
            }
        }
        index += 1;
    }
    proof {
        assert(changes@.take(index as int) =~= changes@);
        reveal(dependency_application_matches);
    };
}

} // verus!
