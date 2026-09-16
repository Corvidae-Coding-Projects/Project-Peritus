//! Completeness lemmas relating semantic DAG certificates to production Kahn traversal.

#[cfg(verus_only)]
use super::cycle_model;
#[cfg(verus_only)]
use crate::ContextNode;
use vstd::prelude::*;

verus! {

pub(super) proof fn full_order_covers(order: Seq<nat>, node_count: nat)
    requires
        cycle_model::order_is_bounded_unique(order, node_count),
        order.len() == node_count,
    ensures forall |index: nat| index < node_count ==> order.contains(index),
{
    reveal(cycle_model::order_is_bounded_unique);
    order.to_set_ensures();
    order.unique_seq_to_set();
    vstd::set_lib::range_set_properties::<nat>(0nat, node_count);
    assert(order.to_set().subset_of(Set::<nat>::range(0nat, node_count))) by {
        assert forall |value: nat| order.to_set().contains(value) implies
            Set::<nat>::range(0nat, node_count).contains(value) by {
            let step = choose |step: int| 0 <= step < order.len()
                && order[step] == value;
            assert(value < node_count);
        }
    }
    vstd::set_lib::lemma_subset_equality(
        order.to_set(),
        Set::<nat>::range(0nat, node_count),
    );
    assert forall |index: nat| index < node_count implies order.contains(index) by {
        assert(Set::<nat>::range(0nat, node_count).contains(index));
        assert(order.to_set().contains(index));
    }
}

proof fn all_removed_means_no_unremoved(removed: Seq<bool>, index: nat)
    requires
        index <= removed.len(),
        forall |current: int| index <= current < removed.len() ==>
            #[trigger] removed[current],
    ensures cycle_model::first_unremoved_from(removed, index).is_none(),
    decreases removed.len() - index,
{
    reveal(cycle_model::first_unremoved_from);
    if index < removed.len() {
        all_removed_means_no_unremoved(removed, index + 1);
    }
}

pub(super) proof fn full_trace_marks_every_node(
    removed: Seq<bool>,
    order: Seq<nat>,
    node_count: nat,
)
    requires
        cycle_model::removal_state_matches(removed, order, node_count),
        order.len() == node_count,
    ensures cycle_model::first_unremoved_from(removed, 0).is_none(),
{
    reveal(cycle_model::removal_state_matches);
    reveal(cycle_model::removal_state_for_order);
    full_order_covers(order, node_count);
    assert forall |index: int| 0 <= index < removed.len() implies
        #[trigger] removed[index] by {
        assert(order.contains(index as nat));
    }
    all_removed_means_no_unremoved(removed, 0);
}

pub(super) proof fn final_state_is_canonical(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
    order: Seq<nat>,
    completed_steps: nat,
    exhausted: bool,
)
    requires
        completed_steps == order.len(),
        completed_steps <= nodes.len(),
        exhausted || completed_steps == nodes.len(),
        exhausted ==> cycle_model::first_removable_from(nodes, removed, 0).is_none(),
        cycle_model::eliminate(
            nodes,
            removed,
            order,
            (nodes.len() - completed_steps) as int,
        ) == cycle_model::canonical_elimination(nodes),
    ensures cycle_model::canonical_elimination(nodes) == (removed, order),
{
    if exhausted {
        assert(cycle_model::eliminate(
            nodes,
            removed,
            order,
            (nodes.len() - completed_steps) as int,
        ) == (removed, order)) by {
            reveal(cycle_model::eliminate);
        }
    } else {
        assert(completed_steps == nodes.len());
        assert(cycle_model::eliminate(
            nodes,
            removed,
            order,
            (nodes.len() - completed_steps) as int,
        ) == (removed, order)) by {
            reveal(cycle_model::eliminate);
        }
    }
}

proof fn first_unremoved_order_step_is_exact(
    order: Seq<nat>,
    removed: Seq<bool>,
    node_count: nat,
    start: nat,
)
    requires
        cycle_model::order_is_bounded_unique(order, node_count),
        removed.len() == node_count,
        start <= order.len(),
    ensures match cycle_model::first_unremoved_order_step(order, removed, start) {
        Some(step) => start <= step < order.len()
            && !removed[order[step as int] as int]
            && (forall |prior: int| start <= prior < step ==>
                removed[#[trigger] order[prior] as int]),
        None => forall |step: int| start <= step < order.len() ==>
            removed[#[trigger] order[step] as int],
    },
    decreases order.len() - start,
{
    reveal(cycle_model::first_unremoved_order_step);
    reveal(cycle_model::order_is_bounded_unique);
    if start < order.len() && removed[order[start as int] as int] {
        first_unremoved_order_step_is_exact(order, removed, node_count, start + 1);
    }
}

proof fn no_remaining_dependent_is_monotonic(
    nodes: Seq<ContextNode>,
    base_removed: Seq<bool>,
    removed: Seq<bool>,
    target: nat,
    source: nat,
)
    requires
        base_removed.len() == nodes.len(),
        removed.len() == nodes.len(),
        target < nodes.len(),
        source <= nodes.len(),
        forall |index: int| 0 <= index < nodes.len()
            && #[trigger] base_removed[index] ==> removed[index],
        cycle_model::first_remaining_dependent_from(
            nodes, base_removed, target, source).is_none(),
    ensures cycle_model::first_remaining_dependent_from(
        nodes, removed, target, source).is_none(),
    decreases nodes.len() - source,
{
    reveal(cycle_model::first_remaining_dependent_from);
    if source < nodes.len() {
        no_remaining_dependent_is_monotonic(
            nodes,
            base_removed,
            removed,
            target,
            source + 1,
        );
    }
}

pub(super) proof fn no_first_removable_means_none_removable(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
    start: nat,
)
    requires
        removed.len() == nodes.len(),
        start <= nodes.len(),
        cycle_model::first_removable_from(nodes, removed, start).is_none(),
    ensures forall |target: int| start <= target < nodes.len() ==>
        !#[trigger] cycle_model::removable(nodes, removed, target as nat),
    decreases nodes.len() - start,
{
    reveal(cycle_model::first_removable_from);
    if start < nodes.len() {
        no_first_removable_means_none_removable(nodes, removed, start + 1);
    }
}

proof fn certificate_has_removable_node(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
    order: Seq<nat>,
)
    requires
        cycle_model::elimination_certificate(nodes, order),
        removed.len() == nodes.len(),
        exists |index: int| 0 <= index < removed.len() && !removed[index],
    ensures exists |target: nat| target < nodes.len()
        && cycle_model::removable(nodes, removed, target),
{
    reveal(cycle_model::elimination_certificate);
    reveal(cycle_model::partial_elimination_certificate);
    full_order_covers(order, nodes.len());
    first_unremoved_order_step_is_exact(order, removed, nodes.len(), 0);
    let remaining = choose |index: int| 0 <= index < removed.len() && !removed[index];
    assert(order.contains(remaining as nat));
    assert(cycle_model::first_unremoved_order_step(order, removed, 0).is_some());
    let step = cycle_model::first_unremoved_order_step(order, removed, 0).unwrap();
    let target = order[step as int];
    let base_removed = cycle_model::removal_state_for_order(
        order.take(step as int),
        nodes.len(),
    );
    assert forall |index: int| 0 <= index < nodes.len()
        && #[trigger] base_removed[index] implies removed[index] by {
        reveal(cycle_model::removal_state_for_order);
        let prior = choose |prior: int| 0 <= prior < order.take(step as int).len()
            && order.take(step as int)[prior] == index as nat;
        assert(prior < step);
        assert(order.take(step as int)[prior] == order[prior]);
    }
    no_remaining_dependent_is_monotonic(
        nodes,
        base_removed,
        removed,
        target,
        0,
    );
    assert(cycle_model::removable(nodes, removed, target)) by {
        reveal(cycle_model::removable);
    }
}

pub(super) proof fn acyclic_state_has_removable_node(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
)
    requires
        cycle_model::acyclic(nodes),
        removed.len() == nodes.len(),
        exists |index: int| 0 <= index < removed.len() && !removed[index],
    ensures exists |target: nat| target < nodes.len()
        && cycle_model::removable(nodes, removed, target),
{
    reveal(cycle_model::acyclic);
    let order = choose |order: Seq<nat>|
        cycle_model::elimination_certificate(nodes, order);
    certificate_has_removable_node(nodes, removed, order);
}

} // verus!
