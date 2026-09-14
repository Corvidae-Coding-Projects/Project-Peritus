//! Exact ordered collection of dependency refresh changes.

use vstd::prelude::*;

#[cfg(verus_only)]
use crate::WorkRecord;
use crate::{SchedulerState, WorkId};

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

} // verus!
