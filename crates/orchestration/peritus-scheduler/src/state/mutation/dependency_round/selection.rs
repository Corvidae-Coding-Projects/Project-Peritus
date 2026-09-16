//! Exact selection identity and pending-target facts for a dependency refresh round.

use vstd::prelude::*;

use crate::{SchedulerState, WorkPhase, WorkRecord};

use super::{
    DependencyChange, change_applicable, change_ids, change_matches, work_id_layout_matches,
};

verus! {
/// An empty exact selection means every source record has the no-change action.
pub(super) proof fn empty_selection_is_fixed_point(
    snapshot: &SchedulerState,
    source: Seq<WorkRecord>,
)
    requires super::changes_match(
        snapshot,
        source,
        Seq::<DependencyChange>::empty(),
    ),
    ensures forall |index: int| #![trigger source[index]]
        0 <= index < source.len() ==>
            super::super::dependency_action::action_matches(
                snapshot,
                &source[index],
                super::DependencyAction::NoChange,
            ),
    decreases source.len(),
{
    reveal(super::changes_match);
    if source.len() > 0 {
        let prefix = source.drop_last();
        let record = source.last();
        assert(super::super::dependency_action::action_matches(
            snapshot,
            &record,
            super::DependencyAction::NoChange,
        )) by {
            if !super::super::dependency_action::action_matches(
                snapshot,
                &record,
                super::DependencyAction::NoChange,
            ) {
                assert(Seq::<DependencyChange>::empty().len() > 0);
            }
        }
        empty_selection_is_fixed_point(snapshot, prefix);
        assert forall |index: int| #![trigger source[index]]
            0 <= index < source.len() implies
                super::super::dependency_action::action_matches(
                    snapshot,
                    &source[index],
                    super::DependencyAction::NoChange,
                ) by {
            if index < prefix.len() {
                assert(prefix[index] == source[index]);
            } else {
                assert(index == source.len() - 1);
                assert(source[index] == record);
            }
        }
    }
}

/// An exact selected action has the inactive source phase required by its mutation.
pub(super) proof fn selected_change_is_applicable(
    snapshot: &SchedulerState,
    record: WorkRecord,
    change: DependencyChange,
)
    requires change_matches(snapshot, record, change),
    ensures super::change_target_matches(record, change),
{
    reveal(change_matches);
    reveal(super::change_target_matches);
    reveal(super::super::dependency_action::action_matches);
    match record.spec_phase() {
        WorkPhase::WaitingDependencies => {},
        WorkPhase::Queued => {},
        _ => {},
    }
}

/// Every selected change has an exact source record in the immutable round snapshot.
pub(super) proof fn change_origin(
    snapshot: &SchedulerState,
    source: Seq<WorkRecord>,
    changes: Seq<DependencyChange>,
    selected: int,
)
    requires
        super::changes_match(snapshot, source, changes),
        0 <= selected < changes.len(),
    ensures exists |source_index: int| #![trigger source[source_index]] {
        &&& 0 <= source_index < source.len()
        &&& change_matches(snapshot, source[source_index], changes[selected])
    },
    decreases source.len(),
{
    reveal(super::changes_match);
    assert(source.len() > 0);
    let prefix = source.drop_last();
    let record = source.last();
    if super::super::dependency_action::action_matches(
        snapshot,
        &record,
        super::DependencyAction::NoChange,
    ) {
        change_origin(snapshot, prefix, changes, selected);
        let source_index = choose |index: int| #![trigger prefix[index]] {
            &&& 0 <= index < prefix.len()
            &&& change_matches(snapshot, prefix[index], changes[selected])
        };
        assert(prefix[source_index] == source[source_index]);
        assert(exists |index: int| #![trigger source[index]] {
            &&& 0 <= index < source.len()
            &&& change_matches(snapshot, source[index], changes[selected])
        }) by {
            assert(source_index < source.len());
        }
    } else {
        assert(changes.len() > 0);
        assert(change_matches(snapshot, record, changes.last()));
        if selected == changes.len() - 1 {
            let source_index = source.len() - 1;
            assert(source[source_index] == record);
            assert(exists |index: int| #![trigger source[index]] {
                &&& 0 <= index < source.len()
                &&& change_matches(snapshot, source[index], changes[selected])
            }) by {
                assert(changes[selected] == changes.last());
            }
        } else {
            assert(0 <= selected < changes.drop_last().len());
            change_origin(snapshot, prefix, changes.drop_last(), selected);
            let source_index = choose |index: int| #![trigger prefix[index]] {
                &&& 0 <= index < prefix.len()
                &&& change_matches(
                    snapshot,
                    prefix[index],
                    changes.drop_last()[selected],
                )
            };
            assert(prefix[source_index] == source[source_index]);
            assert(changes.drop_last()[selected] == changes[selected]);
            assert(exists |index: int| #![trigger source[index]] {
                &&& 0 <= index < source.len()
                &&& change_matches(snapshot, source[index], changes[selected])
            }) by {
                assert(source_index < source.len());
            }
        }
    }
}

proof fn selected_changes_are_unique_from_source(
    snapshot: &SchedulerState,
    source: Seq<WorkRecord>,
    changes: Seq<DependencyChange>,
)
    requires
        crate::verified::work_identities_unique(source),
        super::changes_match(snapshot, source, changes),
    ensures change_ids(changes).no_duplicates(),
    decreases source.len(),
{
    reveal(super::changes_match);
    if source.len() == 0 {
        assert(changes.len() == 0);
        reveal(change_ids);
        return;
    }
    let prefix = source.drop_last();
    let record = source.last();
    assert(crate::verified::work_identities_unique(prefix)) by {
        reveal(crate::verified::work_identities_unique);
    }
    if super::super::dependency_action::action_matches(
        snapshot,
        &record,
        super::DependencyAction::NoChange,
    ) {
        selected_changes_are_unique_from_source(snapshot, prefix, changes);
    } else {
        assert(changes.len() > 0);
        assert(change_matches(snapshot, record, changes.last()));
        let prior = changes.drop_last();
        let change = changes.last();
        selected_changes_are_unique_from_source(snapshot, prefix, prior);
        assert(!change_ids(prior).contains(change.spec_work_id())) by {
            if change_ids(prior).contains(change.spec_work_id()) {
                let selected = choose |index: int| {
                    &&& 0 <= index < change_ids(prior).len()
                    &&& change_ids(prior)[index] == change.spec_work_id()
                };
                assert(0 <= selected < prior.len()) by {
                    reveal(change_ids);
                }
                change_origin(snapshot, prefix, prior, selected);
                let source_index = choose |index: int| #![trigger prefix[index]] {
                    &&& 0 <= index < prefix.len()
                    &&& change_matches(snapshot, prefix[index], prior[selected])
                };
                reveal(change_ids);
                reveal(change_matches);
                assert(prefix[source_index] == source[source_index]);
                assert(record == source[source.len() - 1]);
                assert(source_index < source.len() - 1);
                assert(prior[selected].spec_work_id() == change.spec_work_id());
                assert(prefix[source_index].spec_definition().spec_id()
                    == record.spec_definition().spec_id());
                reveal(crate::verified::work_identities_unique);
                assert(false);
            }
        }
        assert(change_ids(changes)
            =~= change_ids(prior).push(change.spec_work_id())) by {
            reveal(change_ids);
        }
        reveal(Seq::no_duplicates);
    }
}

/// Stable collection order makes the selected change identities unique.
pub(super) proof fn selected_changes_are_unique(
    snapshot: &SchedulerState,
    changes: Seq<DependencyChange>,
)
    requires
        snapshot.spec_reservation_reducer_ready(),
        super::changes_match(snapshot, snapshot.spec_work(), changes),
    ensures change_ids(changes).no_duplicates(),
{
    reveal(SchedulerState::spec_reservation_reducer_ready);
    reveal(crate::verified::reservation_invariant_parts);
    selected_changes_are_unique_from_source(snapshot, snapshot.spec_work(), changes);
}
/// An unchanged pending source record supplies the current-step applicability witness.
pub(super) proof fn pending_change_is_applicable(
    snapshot: &SchedulerState,
    current: &SchedulerState,
    changes: Seq<DependencyChange>,
    index: int,
)
    requires
        0 <= index < changes.len(),
        super::changes_match(snapshot, snapshot.spec_work(), changes),
        change_ids(changes).no_duplicates(),
        work_id_layout_matches(snapshot.spec_work(), current.spec_work()),
        super::unselected_work_unchanged(
            snapshot.spec_work(),
            current.spec_work(),
            change_ids(changes.take(index)),
        ),
    ensures change_applicable(current, changes[index]),
{
    change_origin(snapshot, snapshot.spec_work(), changes, index);
    let source_index = choose |at: int| #![trigger snapshot.spec_work()[at]] {
        &&& 0 <= at < snapshot.spec_work().len()
        &&& change_matches(
            snapshot, snapshot.spec_work()[at], changes[index],
        )
    };
    reveal(Seq::no_duplicates);
    assert(!change_ids(changes.take(index)).contains(
        changes[index].spec_work_id(),
    )) by {
        if change_ids(changes.take(index)).contains(
            changes[index].spec_work_id(),
        ) {
            let prior = choose |at: int| {
                &&& 0 <= at < change_ids(changes.take(index)).len()
                &&& change_ids(changes.take(index))[at]
                    == changes[index].spec_work_id()
            };
            assert(change_ids(changes.take(index)).len() == index) by {
                reveal(change_ids);
            }
            assert(change_ids(changes.take(index))[prior]
                == change_ids(changes)[prior]) by {
                reveal(change_ids);
            }
            assert(change_ids(changes)[index]
                == changes[index].spec_work_id()) by {
                reveal(change_ids);
            }
            assert(change_ids(changes)[prior] != change_ids(changes)[index]);
            assert(false);
        }
    }
    reveal(super::unselected_work_unchanged);
    assert(snapshot.spec_work()[source_index].spec_definition().spec_id()
        == changes[index].spec_work_id()) by {
        reveal(change_matches);
    }
    assert(current.spec_work()[source_index] == snapshot.spec_work()[source_index]);
    selected_change_is_applicable(
        snapshot,
        snapshot.spec_work()[source_index],
        changes[index],
    );
    reveal(change_applicable);
    assert(exists |at: int| #![trigger current.spec_work()[at]] {
        &&& 0 <= at < current.spec_work().len()
        &&& super::change_target_matches(current.spec_work()[at], changes[index])
    }) by {
        assert(source_index < current.spec_work().len());
    }
}

} // verus!
