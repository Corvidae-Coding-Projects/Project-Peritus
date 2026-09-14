//! Ordered composition of exact dependency refresh changes.

use vstd::prelude::*;

use crate::{SchedulerState, WorkRecord};

use super::frame::{
    dependency_step_preserves_ids, dependency_step_preserves_other_record, frame_transitive,
};
use super::{
    DependencyChange, change_ids, dependency_step_matches, dependency_updates_match,
    unselected_work_unchanged, work_id_layout_matches,
};

verus! {

pub(super) proof fn empty_updates(state: SchedulerState)
    ensures
        dependency_updates_match(&state, &state, Seq::<DependencyChange>::empty()),
        unselected_work_unchanged(
            state.spec_work(),
            state.spec_work(),
            Seq::<crate::WorkId>::empty(),
        ),
        work_id_layout_matches(state.spec_work(), state.spec_work()),
{
    let states = Seq::<SchedulerState>::empty().push(state);
    reveal(dependency_updates_match);
    reveal(unselected_work_unchanged);
    reveal(work_id_layout_matches);
    assert(states.len() == 1);
    assert(states[0] == state);
}

proof fn append_update(
    initial: &SchedulerState,
    middle: &SchedulerState,
    after: &SchedulerState,
    prefix: Seq<DependencyChange>,
    change: DependencyChange,
)
    requires
        dependency_updates_match(initial, middle, prefix),
        dependency_step_matches(middle, after, change),
    ensures dependency_updates_match(initial, after, prefix.push(change)),
{
    reveal(dependency_updates_match);
    let states = choose |states: Seq<SchedulerState>| {
        &&& states.len() == prefix.len() + 1
        &&& states[0] == *initial
        &&& states[prefix.len() as int] == *middle
        &&& forall |step: int| #![trigger states[step]] 0 <= step < prefix.len() ==>
            dependency_step_matches(
                &states[step],
                &states[step + 1],
                prefix[step],
            )
    };
    let extended = states.push(*after);
    assert(extended.len() == prefix.push(change).len() + 1);
    assert(extended[0] == *initial);
    assert(extended[prefix.push(change).len() as int] == *after);
    assert forall |step: int| #![trigger extended[step]]
        0 <= step < prefix.push(change).len() implies
            dependency_step_matches(
                &extended[step],
                &extended[step + 1],
                prefix.push(change)[step],
            ) by {
        if step < prefix.len() {
            assert(extended[step] == states[step]);
            assert(extended[step + 1] == states[step + 1]);
            assert(prefix.push(change)[step] == prefix[step]);
        } else {
            assert(step == prefix.len());
            assert(extended[step] == *middle);
            assert(extended[step + 1] == *after);
            assert(prefix.push(change)[step] == change);
        }
    }
    assert(exists |candidate: Seq<SchedulerState>| {
        &&& candidate.len() == prefix.push(change).len() + 1
        &&& candidate[0] == *initial
        &&& candidate[prefix.push(change).len() as int] == *after
        &&& forall |step: int| #![trigger candidate[step]]
            0 <= step < prefix.push(change).len() ==>
                dependency_step_matches(
                    &candidate[step],
                    &candidate[step + 1],
                    prefix.push(change)[step],
                )
    }) by {
    }
}

/// Extends the exact trace and its unchanged-record frame by one selected change.
pub(super) proof fn advance_prefix(
    initial: &SchedulerState,
    middle: &SchedulerState,
    after: &SchedulerState,
    changes: Seq<DependencyChange>,
    index: int,
)
    requires
        0 <= index < changes.len(),
        dependency_updates_match(initial, middle, changes.take(index)),
        dependency_step_matches(middle, after, changes[index]),
        work_id_layout_matches(initial.spec_work(), middle.spec_work()),
        unselected_work_unchanged(
            initial.spec_work(),
            middle.spec_work(),
            change_ids(changes.take(index)),
        ),
        crate::state::mutation::work_update_preserves_other_state(initial, middle),
    ensures
        dependency_updates_match(initial, after, changes.take(index + 1)),
        work_id_layout_matches(initial.spec_work(), after.spec_work()),
        unselected_work_unchanged(
            initial.spec_work(),
            after.spec_work(),
            change_ids(changes.take(index + 1)),
        ),
        crate::state::mutation::work_update_preserves_other_state(initial, after),
{
    hide(dependency_updates_match);
    hide(dependency_step_matches);
    let change = changes[index];
    let prefix = changes.take(index);
    dependency_step_preserves_ids(middle, after, change);
    reveal(work_id_layout_matches);
    assert forall |at: int| #![trigger initial.spec_work()[at]]
        0 <= at < initial.spec_work().len() implies
            after.spec_work()[at].spec_definition().spec_id()
                == initial.spec_work()[at].spec_definition().spec_id() by {
        assert(middle.spec_work()[at].spec_definition().spec_id()
            == initial.spec_work()[at].spec_definition().spec_id());
    }
    append_update(initial, middle, after, prefix, change);
    assert(prefix.push(change) =~= changes.take(index + 1));
    assert(change_ids(changes.take(index + 1))
        =~= change_ids(prefix).push(change.spec_work_id())) by {
        reveal(change_ids);
    }
    assert(unselected_work_unchanged(
        initial.spec_work(),
        after.spec_work(),
        change_ids(changes.take(index + 1)),
    )) by {
        reveal(unselected_work_unchanged);
        assert forall |at: int| #![trigger initial.spec_work()[at]]
            0 <= at < initial.spec_work().len()
                && !change_ids(changes.take(index + 1)).contains(
                    initial.spec_work()[at].spec_definition().spec_id(),
                )
            implies after.spec_work()[at] == initial.spec_work()[at] by {
            vstd::seq_lib::lemma_seq_contains_after_push(
                change_ids(prefix),
                change.spec_work_id(),
                initial.spec_work()[at].spec_definition().spec_id(),
            );
            assert(!change_ids(prefix).contains(
                initial.spec_work()[at].spec_definition().spec_id(),
            ));
            assert(middle.spec_work()[at] == initial.spec_work()[at]);
            assert(initial.spec_work()[at].spec_definition().spec_id()
                != change.spec_work_id());
            dependency_step_preserves_other_record(middle, after, change, at);
        }
    }
    reveal(dependency_step_matches);
    frame_transitive(initial, middle, after);
}

/// The exact trace leaves every record not selected in this round byte-identical.
pub(super) proof fn establish_complete_round(
    initial: &SchedulerState,
    after: &SchedulerState,
    changes: Seq<DependencyChange>,
    changed: bool,
)
    requires
        dependency_updates_match(initial, after, changes),
        unselected_work_unchanged(
            initial.spec_work(), after.spec_work(), change_ids(changes),
        ),
        changed == (changes.len() > 0),
        super::changes_match(initial, initial.spec_work(), changes),
        changes.len() <= initial.spec_work().len(),
    ensures super::dependency_round_matches(initial, after, changed),
{
    reveal(super::dependency_round_matches);
    assert(exists |selected: Seq<DependencyChange>| {
        &&& selected == changes
        &&& selected.len() <= initial.spec_work().len()
        &&& super::changes_match(initial, initial.spec_work(), selected)
        &&& dependency_updates_match(initial, after, selected)
        &&& unselected_work_unchanged(
            initial.spec_work(), after.spec_work(), change_ids(selected),
        )
        &&& changed == (selected.len() > 0)
    }) by {
    }
}

} // verus!
