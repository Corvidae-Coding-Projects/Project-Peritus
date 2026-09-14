//! Exact finite parent closure for production cancellation commands.

use crate::{SchedulerState, WorkId, WorkPhase, WorkRecord};
use vstd::prelude::*;

pub(super) mod command;
mod ids;
#[cfg(verus_only)]
mod non_resurrection;
#[cfg(verus_only)]
mod queue;
#[cfg(test)]
mod tests;
mod update;

verus! {

/// Parent closure uses every retained record, including terminal ancestors.
pub open spec fn parent_closed(work: Seq<WorkRecord>, values: Set<WorkId>) -> bool {
    forall |index: int| #![trigger work[index]] 0 <= index < work.len() ==>
        match work[index].spec_definition().spec_parent() {
            Some(parent) => values.contains(parent) ==>
                values.contains(work[index].spec_definition().spec_id()),
            None => true,
        }
}

/// Membership in the least parent-closed set containing the requested root.
/// This definition is independent of scan order, iteration count, and lifecycle phase.
pub open spec fn reachable(work: Seq<WorkRecord>, root: WorkId, id: WorkId) -> bool {
    forall |values: Set<WorkId>| values.contains(root) && parent_closed(work, values)
        ==> values.contains(id)
}

proof fn root_reachable(work: Seq<WorkRecord>, root: WorkId)
    ensures reachable(work, root, root),
{
}

proof fn child_reachable(work: Seq<WorkRecord>, root: WorkId, parent: WorkId, index: int)
    requires
        0 <= index < work.len(),
        work[index].spec_definition().spec_parent() == Some(parent),
        reachable(work, root, parent),
    ensures reachable(work, root, work[index].spec_definition().spec_id()),
{
    assert forall |values: Set<WorkId>| values.contains(root) && parent_closed(work, values)
        implies values.contains(work[index].spec_definition().spec_id()) by {
        assert(values.contains(parent));
    }
}

fn closure(work: &[WorkRecord], root: WorkId) -> (reached: Vec<WorkId>)
    ensures
        ids::ordered(reached@),
        forall |id: WorkId| reached@.contains(id) == reachable(work@, root, id),
{
    let mut reached = vec![root];
    let ghost universe = work@.map_values(|record: WorkRecord|
        record.spec_definition().spec_id()).to_set().insert(root);
    proof {
        root_reachable(work@, root);
        assert(reached@.to_set().subset_of(universe));
    }
    loop
        invariant
            ids::ordered(reached@),
            reached@.contains(root),
            universe == work@.map_values(|record: WorkRecord|
                record.spec_definition().spec_id()).to_set().insert(root),
            reached@.to_set().subset_of(universe),
            forall |id: WorkId| reached@.contains(id) ==> reachable(work@, root, id),
        decreases universe.difference(reached@.to_set()).len(),
    {
        let ghost before = reached@.to_set();
        let mut changed = false;
        let mut index = 0;
        while index < work.len()
            invariant
                index <= work.len(),
                ids::ordered(reached@),
                reached@.contains(root),
                universe == work@.map_values(|record: WorkRecord|
                    record.spec_definition().spec_id()).to_set().insert(root),
                reached@.to_set().subset_of(universe),
                before.subset_of(reached@.to_set()),
                forall |id: WorkId| reached@.contains(id) ==> reachable(work@, root, id),
                !changed ==> reached@.to_set() == before,
                changed ==> universe.difference(reached@.to_set()).len()
                    < universe.difference(before).len(),
                forall |prior: int| #![trigger work@[prior]] 0 <= prior < index ==>
                    match work@[prior].spec_definition().spec_parent() {
                        Some(parent) => before.contains(parent) ==>
                            reached@.contains(work@[prior].spec_definition().spec_id()),
                        None => true,
                    },
            decreases work.len() - index,
        {
            let record = &work[index];
            match record.spec().parent() {
                Some(parent) if ids::contains(&reached, parent) => {
                    let id = record.spec().id();
                    let ghost previous = reached@.to_set();
                    proof {
                        child_reachable(work@, root, parent, index as int);
                        assert(universe.contains(id)) by {
                            let retained = work@.map_values(|item: WorkRecord|
                                item.spec_definition().spec_id());
                            assert(retained[index as int] == id);
                            assert(retained.contains(id));
                        }
                    }
                    if ids::insert(&mut reached, id) {
                        proof {
                            universe.lemma_set_insert_diff_decreases(previous, id);
                        }
                        changed = true;
                    }
                },
                _ => {},
            }
            index += 1;
        }
        if !changed {
            proof {
                assert(parent_closed(work@, reached@.to_set()));
                assert forall |id: WorkId| reachable(work@, root, id) implies
                    reached@.contains(id) by {
                    assert(reached@.to_set().contains(root));
                }
            }
            return reached;
        }
    }
}

/// Only live retained records are emitted; terminal records still carry parent edges.
pub open spec fn selected(
    work: Seq<WorkRecord>, root: WorkId, descendants: bool, record: WorkRecord,
) -> bool {
    record.spec_phase() != WorkPhase::Terminal
        && (record.spec_definition().spec_id() == root
            || (descendants && reachable(work, root, record.spec_definition().spec_id())))
}

/// Selects the exact pre-state cancellation targets in retained canonical order.
pub(super) fn affected(work: &[WorkRecord], root: WorkId, descendants: bool) -> (result: Vec<WorkId>)
    ensures
        result@ == work@.filter(|record: WorkRecord|
            selected(work@, root, descendants, record)).map_values(|record: WorkRecord|
                record.spec_definition().spec_id()),
        SchedulerState::work_records_ordered(work@) ==> (forall |left: int, right: int|
            0 <= left < right < result.len() ==>
                result@[left].spec_precedes(&result@[right])),
        SchedulerState::work_records_ordered(work@) ==> result@.no_duplicates(),
        forall |id: WorkId| result@.contains(id) ==> exists |index: int|
            0 <= index < work.len() && work@[index].spec_definition().spec_id() == id,
{
    let reached = if descendants { closure(work, root) } else { vec![root] };
    let mut result = Vec::new();
    let mut index = 0;
    let ghost mut origins = Seq::<int>::empty();
    proof {
        reveal(Seq::filter);
        assert(work@.take(0) =~= Seq::<WorkRecord>::empty());
    }
    while index < work.len()
        invariant
            index <= work.len(),
            origins.len() == result.len(),
            forall |at: int| #![trigger origins[at]] 0 <= at < origins.len() ==>
                0 <= origins[at] < index
                    && result@[at] == work@[origins[at]].spec_definition().spec_id(),
            forall |left: int, right: int| 0 <= left < right < origins.len() ==>
                origins[left] < origins[right],
            ids::ordered(reached@),
            forall |id: WorkId| reached@.contains(id)
                == (id == root || (descendants && reachable(work@, root, id))),
            result@ == work@.take(index as int).filter(|record: WorkRecord|
                selected(work@, root, descendants, record)).map_values(|record: WorkRecord|
                    record.spec_definition().spec_id()),
        decreases work.len() - index,
    {
        let record = &work[index];
        let ghost previous = result@;
        if !record.phase().same(WorkPhase::Terminal) && ids::contains(&reached, record.spec().id()) {
            result.push(record.spec().id());
            proof {
                let prior = origins;
                origins = origins.push(index as int);
                assert forall |at: int| #![trigger origins[at]] 0 <= at < origins.len() implies
                    0 <= origins[at] < index + 1
                        && result@[at] == work@[origins[at]].spec_definition().spec_id() by {
                    if at < prior.len() {
                        assert(origins[at] == prior[at]);
                        assert(result@[at] == previous[at]);
                    } else {
                        assert(origins[at] == index);
                    }
                }
                assert forall |left: int, right: int| 0 <= left < right < origins.len() implies
                    origins[left] < origins[right] by {
                    if right < prior.len() {
                        assert(prior[left] < prior[right]);
                    } else {
                        assert(prior[left] < index);
                    }
                }
            };
        }
        proof {
            let prefix = work@.take(index as int);
            prefix.lemma_filter_push(*record, |item: WorkRecord|
                selected(work@, root, descendants, item));
            assert(prefix.push(*record) =~= work@.take(index as int + 1));
            let next = prefix.push(*record).filter(|item: WorkRecord|
                selected(work@, root, descendants, item)).map_values(|item: WorkRecord|
                    item.spec_definition().spec_id());
            if selected(work@, root, descendants, *record) {
                assert(next =~= previous.push(record.spec_definition().spec_id()));
            } else {
                assert(next =~= previous);
            }
        }
        index += 1;
    }
    proof {
        assert(work@.take(index as int) =~= work@);
        if SchedulerState::work_records_ordered(work@) {
            assert forall |left: int, right: int| 0 <= left < right < result.len() implies
                result@[left].spec_precedes(&result@[right]) by {
                assert(0 <= origins[left] < origins[right] < work.len());
            }
            assert forall |left: int, right: int| 0 <= left < right < result.len() implies
                result@[left] != result@[right] by {
                if result@[left] == result@[right] {
                    WorkId::order_irreflexive(&result@[left]);
                }
            }
        }
        assert forall |id: WorkId| result@.contains(id) implies exists |at: int|
            0 <= at < work.len() && work@[at].spec_definition().spec_id() == id by {
            let at = choose |at: int| 0 <= at < result.len() && result@[at] == id;
            assert(work@[origins[at]].spec_definition().spec_id() == id);
        }
    }
    result
}

} // verus!

verus! {

/// Applies the exact selected cancellation sequence while retaining every reservation.
pub(super) fn cancel_retained(
    state: &mut SchedulerState,
    root: WorkId,
    descendants: bool,
) -> (result: (Vec<WorkId>, bool))
    ensures
        result.1,
        result.0@ == old(state).spec_work().filter(|record: WorkRecord|
            selected(old(state).spec_work(), root, descendants, record))
                .map_values(|record: WorkRecord| record.spec_definition().spec_id()),
        update::cancellation_final_matches(old(state).spec_work(), final(state).spec_work(),
            old(state).spec_reservations(), result.0@),
        crate::state::mutation::work_update_preserves_other_state(old(state), final(state)),
        old(state).spec_reservation_invariant() ==> final(state).spec_reservation_invariant(),
        old(state).spec_reservation_reducer_ready() ==> final(state).spec_reservation_reducer_ready(),
        old(state).spec_collections_ordered() ==> final(state).spec_collections_ordered(),
        old(state).spec_reservation_reducer_ready()
                && crate::state::queue::queue_bound(old(state))
            ==> crate::state::queue::queue_bound(final(state)),
        SchedulerState::work_records_ordered(old(state).spec_work()) ==>
            result.0@.no_duplicates(),
{
    let targets = affected(state.work(), root, descendants);
    proof {
        assert forall |at: int| #![trigger targets@[at]] 0 <= at < targets.len() implies
            update::target_exists(state.spec_work(), targets@[at]) by {
            assert(targets@.contains(targets@[at]));
        }
    }
    let complete = update::apply_updates(state, &targets);
    (targets, complete)
}

} // verus!
