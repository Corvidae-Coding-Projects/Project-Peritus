//! Exact ordered collection of dependency refresh changes.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::WorkRecord;
use crate::{SchedulerState, WorkId};

mod apply;
#[cfg(verus_only)]
mod frame;
#[cfg(verus_only)]
mod queue;
#[cfg(verus_only)]
mod selection;
#[cfg(verus_only)]
mod trace;

use apply::apply_dependency_changes;
#[cfg(verus_only)]
use apply::dependency_application_matches;

use super::dependency_action::{DependencyAction, dependency_action};
#[cfg(verus_only)]
use super::dependency_action::{action_matches, actionable_excludes_no_change};

verus! {

/// One work mutation selected from the pre-round dependency snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DependencyChange {
    work_id: WorkId,
    action: DependencyAction,
}

impl DependencyChange {
    /// Returns the mathematical work identity.
    pub closed spec fn spec_work_id(&self) -> WorkId { self.work_id }
    /// Returns the mathematical selected action.
    pub closed spec fn spec_action(&self) -> DependencyAction { self.action }

    const fn new(work_id: WorkId, action: DependencyAction) -> (result: Self)
        ensures
            result.spec_work_id() == work_id,
            result.spec_action() == action,
    {
        Self { work_id, action }
    }

    /// Returns the work identity selected in the immutable scan.
    pub(super) const fn work_id(&self) -> (result: WorkId)
        ensures result == self.spec_work_id(),
    {
        self.work_id
    }

    /// Returns the dependency action selected in the immutable scan.
    pub(super) const fn action(&self) -> (result: DependencyAction)
        ensures result == self.spec_action(),
    {
        self.action
    }
}

/// Work identities in the exact collected order.
pub open spec fn change_ids(changes: Seq<DependencyChange>) -> Seq<WorkId> {
    changes.map_values(|change: DependencyChange| change.spec_work_id())
}

/// The inactive target phase required by one selected change.
pub open spec fn change_target_matches(
    record: WorkRecord,
    change: DependencyChange,
) -> bool {
    &&& record.spec_definition().spec_id() == change.spec_work_id()
    &&& match change.spec_action() {
        DependencyAction::Ready => record.spec_phase() == crate::WorkPhase::WaitingDependencies,
        DependencyAction::Failed { .. } => matches!(
            record.spec_phase(),
            crate::WorkPhase::WaitingDependencies | crate::WorkPhase::Queued
        ),
        DependencyAction::NoChange => false,
    }
}

/// The current state still retains the inactive target required by this change.
pub open spec fn change_applicable(
    state: &SchedulerState,
    change: DependencyChange,
) -> bool {
    exists |index: int| #![trigger state.spec_work()[index]] {
        &&& 0 <= index < state.spec_work().len()
        &&& change_target_matches(state.spec_work()[index], change)
    }
}

pub open spec fn change_matches(
    state: &SchedulerState,
    record: WorkRecord,
    change: DependencyChange,
) -> bool {
    &&& change.spec_work_id() == record.spec_definition().spec_id()
    &&& !matches!(change.spec_action(), DependencyAction::NoChange)
    &&& action_matches(state, &record, change.spec_action())
}

/// Exact stable-order filter/map of actionable work records.
pub open spec fn changes_match(
    state: &SchedulerState,
    source: Seq<WorkRecord>,
    changes: Seq<DependencyChange>,
) -> bool
    decreases source.len(),
{
    if source.len() == 0 {
        changes.len() == 0
    } else {
        let record = source.last();
        let prefix = source.drop_last();
        if action_matches(state, &record, DependencyAction::NoChange) {
            changes_match(state, prefix, changes)
        } else {
            &&& changes.len() > 0
            &&& change_matches(state, record, changes.last())
            &&& changes_match(state, prefix, changes.drop_last())
        }
    }
}

/// Every retained work record is stable under dependency propagation.
pub open spec fn dependency_fixed_point(state: &SchedulerState) -> bool {
    forall |index: int| #![trigger state.spec_work()[index]]
        0 <= index < state.spec_work().len() ==>
            action_matches(
                state,
                &state.spec_work()[index],
                DependencyAction::NoChange,
            )
}

/// Dependency stability depends only on the retained work sequence.
pub(super) proof fn fixed_point_preserved_by_same_work(
    before: &SchedulerState,
    after: &SchedulerState,
)
    requires
        dependency_fixed_point(before),
        after.spec_work() == before.spec_work(),
    ensures dependency_fixed_point(after),
{
    reveal(dependency_fixed_point);
    reveal(action_matches);
}

/// Exact state effect of one applicable dependency refresh change.
pub open spec fn dependency_step_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    change: DependencyChange,
) -> bool {
    &&& change_applicable(before, change)
    &&& crate::state::mutation::work_update_preserves_other_state(before, after)
    &&& match change.spec_action() {
        DependencyAction::Ready => crate::state::mutation::work_phase_update_matches(
            before.spec_work(),
            after.spec_work(),
            change.spec_work_id(),
            crate::WorkPhase::Queued,
            true,
        ),
        DependencyAction::Failed { dependency } =>
            crate::state::mutation::work_terminal_update_matches(
                before.spec_work(),
                after.spec_work(),
                change.spec_work_id(),
                crate::WorkTerminal::DependencyFailed { dependency },
                true,
            ),
        DependencyAction::NoChange => false,
    }
}

/// Exact retained-index identity correspondence across lifecycle-only updates.
pub open spec fn work_id_layout_matches(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
) -> bool {
    &&& before.len() == after.len()
    &&& forall |index: int| #![trigger before[index]] 0 <= index < before.len() ==>
        after[index].spec_definition().spec_id()
            == before[index].spec_definition().spec_id()
}

/// Records not selected in the supplied prefix remain byte-identical.
pub open spec fn unselected_work_unchanged(
    before: Seq<WorkRecord>,
    after: Seq<WorkRecord>,
    selected: Seq<WorkId>,
) -> bool {
    &&& before.len() == after.len()
    &&& forall |index: int| #![trigger before[index]] 0 <= index < before.len()
            && !selected.contains(before[index].spec_definition().spec_id()) ==>
        after[index] == before[index]
}

/// Exact finite state trace in the immutable collection order.
pub open spec fn dependency_updates_match(
    before: &SchedulerState,
    after: &SchedulerState,
    changes: Seq<DependencyChange>,
) -> bool {
    exists |states: Seq<SchedulerState>| {
        &&& states.len() == changes.len() + 1
        &&& states[0] == *before
        &&& states[changes.len() as int] == *after
        &&& forall |step: int| #![trigger states[step]] 0 <= step < changes.len() ==>
            dependency_step_matches(
                &states[step],
                &states[step + 1],
                changes[step],
            )
    }
}

/// Complete result of one immutable-snapshot dependency refresh round.
pub open spec fn dependency_round_matches(
    before: &SchedulerState,
    after: &SchedulerState,
    changed: bool,
) -> bool {
    exists |changes: Seq<DependencyChange>| {
        &&& changes.len() <= before.spec_work().len()
        &&& changes_match(before, before.spec_work(), changes)
        &&& dependency_updates_match(before, after, changes)
        &&& unselected_work_unchanged(
            before.spec_work(), after.spec_work(), change_ids(changes),
        )
        &&& changed == (changes.len() > 0)
    }
}

/// Collects the exact production refresh actions from one immutable state snapshot.
pub(super) fn collect_dependency_changes(
    state: &SchedulerState,
) -> (changes: Vec<DependencyChange>)
    ensures
        changes@.len() <= state.spec_work().len(),
        state.spec_work_ordered() ==>
            changes_match(state, state.spec_work(), changes@),
{
    let work = state.work();
    let mut changes = Vec::new();
    let mut index = 0;
    proof {
        assert(work@.take(0) =~= Seq::<WorkRecord>::empty());
        reveal(changes_match);
    }
    while index < work.len()
        invariant
            index <= work@.len(),
            work@ == state.spec_work(),
            changes@.len() <= index,
            state.spec_work_ordered() ==>
                changes_match(state, work@.take(index as int), changes@),
        decreases work@.len() - index,
    {
        let record = &work[index];
        let ghost prefix = work@.take(index as int);
        let ghost next = work@.take(index as int + 1);
        let action = dependency_action(state, record);
        match action {
            DependencyAction::NoChange => {
                proof {
                    if state.spec_work_ordered() {
                        assert(action_matches(state, record, DependencyAction::NoChange));
                        assert(next =~= prefix.push(*record));
                        assert(next.len() > 0);
                        assert(next.last() == *record);
                        assert(next.drop_last() =~= prefix);
                        assert(changes_match(state, prefix, changes@));
                        assert(changes_match(state, next, changes@)) by {
                            reveal(changes_match);
                        }
                    }
                };
            },
            DependencyAction::Ready | DependencyAction::Failed { .. } => {
                let ghost previous = changes@;
                let change = DependencyChange::new(record.spec().id(), action);
                changes.push(change);
                proof {
                    if state.spec_work_ordered() {
                        assert(change_matches(state, *record, change));
                        assert(action != DependencyAction::NoChange);
                        actionable_excludes_no_change(state, record, action);
                        assert(!action_matches(state, record, DependencyAction::NoChange));
                        assert(changes@ =~= previous.push(change));
                        assert(changes@.len() > 0);
                        assert(changes@.last() == change);
                        assert(changes@.drop_last() =~= previous);
                        assert(next =~= prefix.push(*record));
                        assert(next.len() > 0);
                        assert(next.last() == *record);
                        assert(next.drop_last() =~= prefix);
                        assert(changes_match(state, prefix, previous));
                        assert(changes_match(state, next, changes@)) by {
                            reveal(changes_match);
                        }
                    }
                };
            },
        }
        index += 1;
    }
    proof {
        assert(work@.take(index as int) =~= work@);
    }
    changes
}

/// Collects and applies exactly one immutable dependency-refresh round.
///
/// `changed` reports whether this round selected at least one mutation. A caller may iterate
/// rounds to a fixed point separately; this function makes no fixed-point claim.
pub(super) fn apply_dependency_round(
    state: &mut SchedulerState,
) -> (changed: bool)
    ensures
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
            ==> {
                &&& dependency_round_matches(old(state), final(state), changed)
                &&& final(state).spec_reservation_reducer_ready()
                &&& final(state).spec_collections_ordered()
            },
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
                && crate::state::queue::queue_bound(old(state))
            ==> crate::state::queue::queue_bound(final(state)),
        crate::state::mutation::work_update_preserves_other_state(
            old(state), final(state),
        ),
        work_id_layout_matches(old(state).spec_work(), final(state).spec_work()),
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
                && changed
            ==> super::dependency_measure::dependency_measure(final(state).spec_work())
                < super::dependency_measure::dependency_measure(old(state).spec_work()),
        old(state).spec_reservation_reducer_ready()
                && old(state).spec_collections_ordered()
                && !changed
            ==> dependency_fixed_point(final(state)),
{
    let ghost initial = *state;
    let changes = collect_dependency_changes(state);
    apply_dependency_changes(state, &changes);
    proof {
        reveal(dependency_application_matches);
        if initial.spec_reservation_reducer_ready()
            && initial.spec_collections_ordered()
        {
            trace::establish_complete_round(
                &initial,
                state,
                changes@,
                changes@.len() > 0,
            );
            if changes@.len() == 0 {
                assert(changes@ =~= Seq::<DependencyChange>::empty());
                assert(*state == initial);
                assert(changes_match(
                    &initial,
                    initial.spec_work(),
                    Seq::<DependencyChange>::empty(),
                ));
                selection::empty_selection_is_fixed_point(
                    &initial,
                    initial.spec_work(),
                );
            }
        }
    };
    !changes.is_empty()
}

} // verus!
