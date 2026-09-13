//! Finite specification of canonical graph identity and dependency admission.

#[cfg(verus_only)]
use crate::{ContextErrorKind, ContextNode, ContextNodeId};
#[cfg(verus_only)]
use core::cmp::Ordering;
use vstd::prelude::*;

verus! {

/// First canonical node-identity error at or after `index`.
pub open spec fn first_node_order_error(
    nodes: Seq<ContextNode>,
    index: nat,
) -> Option<(ContextErrorKind, ContextNodeId)>
    decreases nodes.len() - index,
{
    if index >= nodes.len() {
        None
    } else {
        match nodes[index as int - 1].spec_id().spec_order(&nodes[index as int].spec_id()) {
            Ordering::Less => first_node_order_error(nodes, index + 1),
            Ordering::Equal => Some((
                ContextErrorKind::DuplicateValue,
                nodes[index as int].spec_id(),
            )),
            Ordering::Greater => Some((
                ContextErrorKind::NonCanonicalOrder,
                nodes[index as int].spec_id(),
            )),
        }
    }
}

/// Whether node identities are in strict canonical order.
pub open spec fn nodes_canonical(nodes: Seq<ContextNode>) -> bool {
    first_node_order_error(nodes, 1).is_none()
}

/// First exact node identity at or after `index`.
pub open spec fn first_node_index_from(
    nodes: Seq<ContextNode>,
    id: ContextNodeId,
    index: nat,
) -> Option<nat>
    decreases nodes.len() - index,
{
    if index >= nodes.len() {
        None
    } else if nodes[index as int].spec_id().spec_matches(&id) {
        Some(index)
    } else {
        first_node_index_from(nodes, id, index + 1)
    }
}

/// Whether an exact node identity occurs in the supplied sequence.
pub open spec fn node_exists(nodes: Seq<ContextNode>, id: ContextNodeId) -> bool {
    first_node_index_from(nodes, id, 0).is_some()
}

/// Exact result of full node lookup.
pub open spec fn node_index_result(
    nodes: Seq<ContextNode>,
    id: ContextNodeId,
    result: Option<usize>,
) -> bool {
    match result {
        Some(index) => first_node_index_from(nodes, id, 0) == Some(index as nat),
        None => first_node_index_from(nodes, id, 0).is_none(),
    }
}

/// First missing dependency for one node at or after `index`.
pub open spec fn first_missing_for_node(
    nodes: Seq<ContextNode>,
    owner: ContextNodeId,
    dependencies: Seq<ContextNodeId>,
    index: nat,
) -> Option<(ContextNodeId, ContextNodeId)>
    decreases dependencies.len() - index,
{
    if index >= dependencies.len() {
        None
    } else if !node_exists(nodes, dependencies[index as int]) {
        Some((owner, dependencies[index as int]))
    } else {
        first_missing_for_node(nodes, owner, dependencies, index + 1)
    }
}

/// First missing dependency in node and dependency traversal order.
pub open spec fn first_missing_dependency(
    nodes: Seq<ContextNode>,
    index: nat,
) -> Option<(ContextNodeId, ContextNodeId)>
    decreases nodes.len() - index,
{
    if index >= nodes.len() {
        None
    } else {
        match first_missing_for_node(
            nodes,
            nodes[index as int].spec_id(),
            nodes[index as int].spec_dependencies(),
            0,
        ) {
            Some(pair) => Some(pair),
            None => first_missing_dependency(nodes, index + 1),
        }
    }
}

/// Whether every direct dependency resolves to an exact graph node.
pub open spec fn dependencies_exist(nodes: Seq<ContextNode>) -> bool {
    first_missing_dependency(nodes, 0).is_none()
}

proof fn missing_dependency_suffix_is_empty(
    nodes: Seq<ContextNode>,
    index: nat,
)
    requires
        first_missing_dependency(nodes, 0).is_none(),
        index <= nodes.len(),
    ensures first_missing_dependency(nodes, index).is_none(),
    decreases index,
{
    if index > 0 {
        missing_dependency_suffix_is_empty(nodes, (index - 1) as nat);
        reveal(first_missing_dependency);
    }
}

proof fn missing_for_node_suffix_is_empty(
    nodes: Seq<ContextNode>,
    owner: ContextNodeId,
    dependencies: Seq<ContextNodeId>,
    index: nat,
)
    requires
        first_missing_for_node(nodes, owner, dependencies, 0).is_none(),
        index <= dependencies.len(),
    ensures first_missing_for_node(nodes, owner, dependencies, index).is_none(),
    decreases index,
{
    if index > 0 {
        missing_for_node_suffix_is_empty(
            nodes,
            owner,
            dependencies,
            (index - 1) as nat,
        );
        reveal(first_missing_for_node);
    }
}

pub(super) proof fn admitted_dependency_exists(
    nodes: Seq<ContextNode>,
    source: nat,
    dependency: nat,
)
    requires
        dependencies_exist(nodes),
        source < nodes.len(),
        dependency < nodes[source as int].spec_dependencies().len(),
    ensures node_exists(
        nodes,
        nodes[source as int].spec_dependencies()[dependency as int],
    ),
{
    reveal(dependencies_exist);
    missing_dependency_suffix_is_empty(nodes, source);
    reveal(first_missing_dependency);
    let dependencies = nodes[source as int].spec_dependencies();
    missing_for_node_suffix_is_empty(
        nodes,
        nodes[source as int].spec_id(),
        dependencies,
        dependency,
    );
    reveal(first_missing_for_node);
}

proof fn order_error_suffix_is_empty(
    nodes: Seq<ContextNode>,
    index: nat,
)
    requires
        first_node_order_error(nodes, 1).is_none(),
        1 <= index <= nodes.len(),
    ensures first_node_order_error(nodes, index).is_none(),
    decreases index - 1,
{
    if index > 1 {
        order_error_suffix_is_empty(nodes, (index - 1) as nat);
        reveal(first_node_order_error);
    }
}

proof fn canonical_adjacent(
    nodes: Seq<ContextNode>,
    index: nat,
)
    requires
        nodes_canonical(nodes),
        1 <= index < nodes.len(),
    ensures
        nodes[index as int - 1].spec_id().spec_order(
            &nodes[index as int].spec_id(),
        ) == Ordering::Less,
{
    reveal(nodes_canonical);
    order_error_suffix_is_empty(nodes, index);
    reveal(first_node_order_error);
    match nodes[index as int - 1].spec_id().spec_order(
        &nodes[index as int].spec_id(),
    ) {
        Ordering::Less => {}
        Ordering::Equal => { assert(false); }
        Ordering::Greater => { assert(false); }
    }
}

proof fn canonical_pair(
    nodes: Seq<ContextNode>,
    left: nat,
    right: nat,
)
    requires
        nodes_canonical(nodes),
        left < right < nodes.len(),
    ensures nodes[left as int].spec_id().spec_order(
        &nodes[right as int].spec_id(),
    ) == Ordering::Less,
    decreases right - left,
{
    canonical_adjacent(nodes, right);
    if left + 1 < right {
        canonical_pair(nodes, left, (right - 1) as nat);
        ContextNodeId::order_transitive(
            &nodes[left as int].spec_id(),
            &nodes[right as int - 1].spec_id(),
            &nodes[right as int].spec_id(),
        );
    }
}

pub(super) proof fn canonical_match_is_unique(
    nodes: Seq<ContextNode>,
    id: ContextNodeId,
    found: nat,
    other: nat,
)
    requires
        nodes_canonical(nodes),
        found < nodes.len(),
        other < nodes.len(),
        nodes[found as int].spec_id().spec_matches(&id),
        nodes[other as int].spec_id().spec_matches(&id),
    ensures found == other,
{
    ContextNodeId::matches_implies_equal(&nodes[found as int].spec_id(), &id);
    ContextNodeId::matches_implies_equal(&nodes[other as int].spec_id(), &id);
    if found < other {
        canonical_pair(nodes, found, other);
        ContextNodeId::order_reflexive(&id);
        assert(false);
    } else if other < found {
        canonical_pair(nodes, other, found);
        ContextNodeId::order_reflexive(&id);
        assert(false);
    }
}

} // verus!
