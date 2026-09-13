//! Exact edge-occurrence and processed-source count model.

#[cfg(verus_only)]
use super::cycle_count_model::{remaining_count_from};
#[cfg(verus_only)]
use super::cycle_model;
#[cfg(verus_only)]
use crate::{ContextNode, ContextNodeId};
use vstd::prelude::*;

verus! {

/// Number of matching dependency identities at or after one dependency index.
pub open spec fn dependency_match_count_from(
    dependencies: Seq<ContextNodeId>,
    id: ContextNodeId,
    index: nat,
) -> nat
    decreases dependencies.len() - index,
{
    if index >= dependencies.len() {
        0
    } else {
        (if dependencies[index as int].spec_matches(&id) { 1nat } else { 0nat })
            + dependency_match_count_from(dependencies, id, index + 1)
    }
}

/// Number of direct dependent sources already processed during count construction.
pub open spec fn processed_source_count(
    nodes: Seq<ContextNode>,
    target: nat,
    source: nat,
) -> nat
    decreases source,
{
    if source == 0 {
        0
    } else {
        processed_source_count(nodes, target, (source - 1) as nat)
            + if cycle_model::node_depends_on(nodes, (source - 1) as nat, target) {
                1nat
            } else {
                0nat
            }
    }
}

/// A processed prefix contributes at most one count per source.
pub proof fn processed_source_count_is_bounded(
    nodes: Seq<ContextNode>,
    target: nat,
    source: nat,
)
    ensures processed_source_count(nodes, target, source) <= source,
    decreases source,
{
    reveal(processed_source_count);
    if source > 0 {
        processed_source_count_is_bounded(nodes, target, (source - 1) as nat);
    }
}

proof fn positive_match_count_has_witness(
    dependencies: Seq<ContextNodeId>,
    id: ContextNodeId,
    index: nat,
)
    requires
        index <= dependencies.len(),
        dependency_match_count_from(dependencies, id, index) > 0,
    ensures exists |found: nat| index <= found < dependencies.len()
        && dependencies[found as int].spec_matches(&id),
    decreases dependencies.len() - index,
{
    reveal(dependency_match_count_from);
    if index < dependencies.len()
        && !dependencies[index as int].spec_matches(&id)
    {
        positive_match_count_has_witness(dependencies, id, index + 1);
    }
}

proof fn matching_index_makes_count_positive_from(
    dependencies: Seq<ContextNodeId>,
    id: ContextNodeId,
    found: nat,
    start: nat,
)
    requires
        start <= found < dependencies.len(),
        dependencies[found as int].spec_matches(&id),
    ensures dependency_match_count_from(dependencies, id, start) > 0,
    decreases found - start,
{
    reveal(dependency_match_count_from);
    if start < found {
        matching_index_makes_count_positive_from(dependencies, id, found, start + 1);
    }
}

/// A matching dependency at a known index makes the complete occurrence count positive.
pub proof fn matching_index_makes_full_count_positive(
    dependencies: Seq<ContextNodeId>,
    id: ContextNodeId,
    found: nat,
)
    requires
        found < dependencies.len(),
        dependencies[found as int].spec_matches(&id),
    ensures dependency_match_count_from(dependencies, id, 0) > 0,
{
    matching_index_makes_count_positive_from(dependencies, id, found, 0);
}

proof fn match_count_is_at_most_one(
    node: &ContextNode,
    id: ContextNodeId,
    index: nat,
)
    requires
        node.invariant(),
        index <= node.spec_dependencies().len(),
    ensures dependency_match_count_from(node.spec_dependencies(), id, index) <= 1,
    decreases node.spec_dependencies().len() - index,
{
    reveal(dependency_match_count_from);
    if index < node.spec_dependencies().len() {
        match_count_is_at_most_one(node, id, index + 1);
        if node.spec_dependencies()[index as int].spec_matches(&id)
            && dependency_match_count_from(node.spec_dependencies(), id, index + 1) > 0
        {
            positive_match_count_has_witness(node.spec_dependencies(), id, index + 1);
            let other = choose |other: nat| index + 1 <= other
                && other < node.spec_dependencies().len()
                && node.spec_dependencies()[other as int].spec_matches(&id);
            assert(node.spec_dependencies()[index as int].spec_matches(
                &node.spec_dependencies()[other as int],
            ));
            node.dependency_match_is_unique(index, other);
            assert(false);
        }
    }
}

proof fn match_count_positive_iff_contains(
    dependencies: Seq<ContextNodeId>,
    id: ContextNodeId,
    index: nat,
)
    requires index <= dependencies.len(),
    ensures
        (dependency_match_count_from(dependencies, id, index) > 0)
            == cycle_model::dependency_contains_from(dependencies, id, index),
    decreases dependencies.len() - index,
{
    reveal(dependency_match_count_from);
    reveal(cycle_model::dependency_contains_from);
    if index < dependencies.len() {
        match_count_positive_iff_contains(dependencies, id, index + 1);
    }
}

/// A checked node's canonical dependencies contribute exactly one or zero to an edge count.
pub(super) proof fn dependency_match_count_is_indicator(
    node: &ContextNode,
    id: ContextNodeId,
)
    requires node.invariant(),
    ensures
        dependency_match_count_from(node.spec_dependencies(), id, 0)
            == if cycle_model::dependency_contains_from(
                node.spec_dependencies(), id, 0,
            ) { 1nat } else { 0nat },
{
    match_count_is_at_most_one(node, id, 0);
    match_count_positive_iff_contains(node.spec_dependencies(), id, 0);
}

/// Processed source counts and the unprocessed suffix partition the initial exact count.
pub proof fn processed_sources_partition_initial(
    nodes: Seq<ContextNode>,
    target: nat,
    source: nat,
)
    requires
        target < nodes.len(),
        source <= nodes.len(),
    ensures
        processed_source_count(nodes, target, source)
            + remaining_count_from(
                nodes,
                cycle_model::initial_removed(nodes.len()),
                target,
                source,
            )
            == remaining_count_from(
                nodes,
                cycle_model::initial_removed(nodes.len()),
                target,
                0,
            ),
    decreases source,
{
    if source > 0 {
        processed_sources_partition_initial(nodes, target, (source - 1) as nat);
        reveal(processed_source_count);
        reveal(remaining_count_from);
        reveal(cycle_model::initial_removed);
    }
}

} // verus!
