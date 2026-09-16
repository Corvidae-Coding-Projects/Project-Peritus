//! Finite Kahn elimination model and semantic acyclicity certificate.

#[cfg(verus_only)]
use crate::{ContextNode, ContextNodeId};
use vstd::prelude::*;

verus! {

/// Whether a dependency identity occurs at or after `index`.
pub open spec fn dependency_contains_from(
    dependencies: Seq<ContextNodeId>,
    id: ContextNodeId,
    index: nat,
) -> bool
    decreases dependencies.len() - index,
{
    index < dependencies.len()
        && (dependencies[index as int].spec_matches(&id)
            || dependency_contains_from(dependencies, id, index + 1))
}

/// Whether the node at `source` directly depends on the node at `target`.
pub open spec fn node_depends_on(
    nodes: Seq<ContextNode>,
    source: nat,
    target: nat,
) -> bool {
    source < nodes.len() && target < nodes.len()
        && dependency_contains_from(
            nodes[source as int].spec_dependencies(),
            nodes[target as int].spec_id(),
            0,
        )
}

/// First remaining source that directly depends on `target`.
pub open spec fn first_remaining_dependent_from(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
    target: nat,
    source: nat,
) -> Option<nat>
    decreases nodes.len() - source,
{
    if source >= nodes.len() {
        None
    } else if !removed[source as int] && node_depends_on(nodes, source, target) {
        Some(source)
    } else {
        first_remaining_dependent_from(nodes, removed, target, source + 1)
    }
}

/// Whether one remaining node can be removed as a Kahn sink.
pub open spec fn removable(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
    target: nat,
) -> bool {
    removed.len() == nodes.len()
        && target < nodes.len()
        && !removed[target as int]
        && first_remaining_dependent_from(nodes, removed, target, 0).is_none()
}

/// First removable node in canonical storage order.
pub open spec fn first_removable_from(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
    index: nat,
) -> Option<nat>
    decreases nodes.len() - index,
{
    if index >= nodes.len() {
        None
    } else if removable(nodes, removed, index) {
        Some(index)
    } else {
        first_removable_from(nodes, removed, index + 1)
    }
}

/// First node not yet removed in canonical storage order.
pub open spec fn first_unremoved_from(
    removed: Seq<bool>,
    index: nat,
) -> Option<nat>
    decreases removed.len() - index,
{
    if index >= removed.len() {
        None
    } else if !removed[index as int] {
        Some(index)
    } else {
        first_unremoved_from(removed, index + 1)
    }
}

/// First trace step whose node remains present in a supplied removal state.
pub open spec fn first_unremoved_order_step(
    order: Seq<nat>,
    removed: Seq<bool>,
    step: nat,
) -> Option<nat>
    decreases order.len() - step,
{
    if step >= order.len() {
        None
    } else if !removed[order[step as int] as int] {
        Some(step)
    } else {
        first_unremoved_order_step(order, removed, step + 1)
    }
}

/// Whether a removal trace contains only distinct, in-range node indexes.
pub open spec fn order_is_bounded_unique(order: Seq<nat>, node_count: nat) -> bool {
    (forall |step: int| 0 <= step < order.len() ==>
        #[trigger] order[step] < node_count)
        && order.no_duplicates()
}

/// Boolean removal state represented by an ordered removal trace.
pub open spec fn removal_state_for_order(
    order: Seq<nat>,
    node_count: nat,
) -> Seq<bool> {
    Seq::new(node_count, |index: int| order.contains(index as nat))
}

/// Boolean removal state exactly represented by the ordered removal trace.
pub open spec fn removal_state_matches(
    removed: Seq<bool>,
    order: Seq<nat>,
    node_count: nat,
) -> bool {
    order_is_bounded_unique(order, node_count)
        && removed == removal_state_for_order(order, node_count)
}

/// Every trace step removes a node with no dependent remaining at that step.
pub open spec fn partial_elimination_certificate(
    nodes: Seq<ContextNode>,
    order: Seq<nat>,
) -> bool {
    order_is_bounded_unique(order, nodes.len())
        && forall |step: int| 0 <= step < order.len() ==>
            first_remaining_dependent_from(
                nodes,
                removal_state_for_order(order.take(step), nodes.len()),
                #[trigger] order[step],
                0,
            ).is_none()
}

/// A complete finite elimination trace witnessing graph acyclicity.
pub open spec fn elimination_certificate(
    nodes: Seq<ContextNode>,
    order: Seq<nat>,
) -> bool {
    order.len() == nodes.len() && partial_elimination_certificate(nodes, order)
}

/// Declarative acyclicity witnessed by a complete finite sink-elimination order.
pub open spec fn acyclic(nodes: Seq<ContextNode>) -> bool {
    exists |order: Seq<nat>| elimination_certificate(nodes, order)
}

/// Deterministic continuation of canonical Kahn elimination.
pub open spec fn eliminate(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
    order: Seq<nat>,
    fuel: int,
) -> (Seq<bool>, Seq<nat>)
    decreases fuel,
{
    if fuel <= 0 {
        (removed, order)
    } else {
        match first_removable_from(nodes, removed, 0) {
            Some(index) => eliminate(
                nodes,
                removed.update(index as int, true),
                order.push(index),
                fuel - 1,
            ),
            None => (removed, order),
        }
    }
}

/// Initial all-present removal state.
pub open spec fn initial_removed(node_count: nat) -> Seq<bool> {
    Seq::new(node_count, |index: int| false)
}

/// The initial all-present state is represented by the empty trace.
pub proof fn initial_state_matches(node_count: nat)
    ensures removal_state_matches(initial_removed(node_count), Seq::empty(), node_count),
{
    assert(initial_removed(node_count) =~= removal_state_for_order(Seq::empty(), node_count));
}

/// A completed scan means every entry in the removal state is marked.
pub proof fn first_unremoved_none_means_all_removed(
    removed: Seq<bool>,
    index: nat,
)
    requires first_unremoved_from(removed, index).is_none(),
    ensures forall |current: int| index <= current < removed.len() ==>
        #[trigger] removed[current],
    decreases removed.len() - index,
{
    reveal(first_unremoved_from);
    if index < removed.len() {
        first_unremoved_none_means_all_removed(removed, index + 1);
    }
}

/// A distinct bounded trace that marks every node contains exactly one step per node.
pub proof fn complete_trace_has_full_length(
    removed: Seq<bool>,
    order: Seq<nat>,
    node_count: nat,
)
    requires
        removal_state_matches(removed, order, node_count),
        first_unremoved_from(removed, 0).is_none(),
    ensures order.len() == node_count,
{
    reveal(removal_state_matches);
    reveal(order_is_bounded_unique);
    reveal(removal_state_for_order);
    first_unremoved_none_means_all_removed(removed, 0);
    order.to_set_ensures();
    order.unique_seq_to_set();
    vstd::set_lib::range_set_properties::<nat>(0nat, node_count);
    assert(order.to_set() =~= Set::<nat>::range(0nat, node_count)) by {
        assert forall |value: nat| order.to_set().contains(value) <==>
            Set::<nat>::range(0nat, node_count).contains(value) by {
            if order.to_set().contains(value) {
                let step = choose |step: int| 0 <= step < order.len()
                    && order[step] == value;
                assert(value < node_count);
            } else if Set::<nat>::range(0nat, node_count).contains(value) {
                assert(value < node_count);
                assert(removed[value as int]);
                assert(order.contains(value));
            }
        }
    }
}

/// Appending one actually removable node preserves the trace and certificate invariants.
pub proof fn append_removable(
    nodes: Seq<ContextNode>,
    removed: Seq<bool>,
    order: Seq<nat>,
    target: nat,
)
    requires
        removal_state_matches(removed, order, nodes.len()),
        partial_elimination_certificate(nodes, order),
        removable(nodes, removed, target),
    ensures
        removal_state_matches(
            removed.update(target as int, true),
            order.push(target),
            nodes.len(),
        ),
        partial_elimination_certificate(nodes, order.push(target)),
{
    reveal(removal_state_matches);
    reveal(order_is_bounded_unique);
    reveal(removal_state_for_order);
    reveal(partial_elimination_certificate);
    reveal(removable);

    assert(!order.contains(target));
    assert(order.push(target).no_duplicates());
    assert forall |index: int| 0 <= index < nodes.len() implies
        removed.update(target as int, true)[index]
            == removal_state_for_order(order.push(target), nodes.len())[index] by {
        vstd::seq_lib::lemma_seq_contains_after_push(
            order,
            target,
            index as nat,
        );
        if index == target {
            assert(removed.update(target as int, true)[index]);
        } else {
            assert(removed.update(target as int, true)[index] == removed[index]);
        }
    }
    assert(removed.update(target as int, true) =~=
        removal_state_for_order(order.push(target), nodes.len()));

    assert forall |step: int| 0 <= step < order.push(target).len() implies
        first_remaining_dependent_from(
            nodes,
            removal_state_for_order(order.push(target).take(step), nodes.len()),
            #[trigger] order.push(target)[step],
            0,
        ).is_none() by {
        if step < order.len() {
            assert(order.push(target).take(step) =~= order.take(step));
        } else {
            assert(step == order.len());
            assert(order.push(target).take(step) =~= order);
        }
    }
}

/// Complete canonical Kahn result.
pub open spec fn canonical_elimination(
    nodes: Seq<ContextNode>,
) -> (Seq<bool>, Seq<nat>) {
    eliminate(nodes, initial_removed(nodes.len()), Seq::empty(), nodes.len() as int)
}

/// Exact diagnostic identity from the final deterministic removal state.
pub open spec fn canonical_cycle_node(
    nodes: Seq<ContextNode>,
) -> Option<ContextNodeId> {
    let removed = canonical_elimination(nodes).0;
    match first_unremoved_from(removed, 0) {
        Some(index) => Some(nodes[index as int].spec_id()),
        None => None,
    }
}

/// Exact diagnostic returned from the final deterministic removal state.
pub open spec fn cycle_result(
    nodes: Seq<ContextNode>,
    result: Option<ContextNodeId>,
) -> bool {
    result == canonical_cycle_node(nodes)
}

} // verus!
