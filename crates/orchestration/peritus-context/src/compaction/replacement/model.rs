//! Logical model for exact compaction replacement.

#[cfg(verus_only)]
use super::super::ValidatedCompaction;
#[cfg(verus_only)]
use crate::{ContextGraph, ContextNodeId};
use vstd::prelude::*;

verus! {
pub open spec fn contains_id(values: Seq<ContextNodeId>, target: ContextNodeId) -> bool {
    exists |index: int| #![trigger values[index]]
        0 <= index < values.len() && values[index].spec_matches(&target)
}

pub(super) proof fn contains_id_is_sequence_membership(values: Seq<ContextNodeId>, target: ContextNodeId)
    ensures contains_id(values, target) == values.contains(target),
{
    reveal(contains_id);
    if contains_id(values, target) {
        let index = choose |index: int| #![trigger values[index]]
            0 <= index < values.len() && values[index].spec_matches(&target);
        ContextNodeId::matches_implies_equal(&values[index], &target);
    }
    if values.contains(target) {
        let index = choose |index: int| 0 <= index < values.len() && values[index] == target;
        assert(values[index].spec_matches(&target));
    }
}

pub(super) proof fn ids_match_iff_equal(left: ContextNodeId, right: ContextNodeId)
    ensures left.spec_matches(&right) == (left == right),
{
    if left.spec_matches(&right) {
        ContextNodeId::matches_implies_equal(&left, &right);
    }
    if left == right {
        reveal(ContextNodeId::spec_matches);
    }
}

pub open spec fn rewritten_dependency_contains(
    dependencies: Seq<ContextNodeId>,
    source_ids: Seq<ContextNodeId>,
    output_id: ContextNodeId,
    target: ContextNodeId,
) -> bool
    decreases dependencies.len(),
{
    if dependencies.len() == 0 {
        false
    } else {
        rewritten_dependency_contains(
            dependencies.drop_last(), source_ids, output_id, target,
        ) || (if contains_id(source_ids, dependencies.last()) {
                output_id
            } else {
                dependencies.last()
            }) == target
    }
}

pub open spec fn source_external_dependency_contains(
    dependencies: Seq<ContextNodeId>,
    source_ids: Seq<ContextNodeId>,
    target: ContextNodeId,
) -> bool
    decreases dependencies.len(),
{
    if dependencies.len() == 0 {
        false
    } else if contains_id(source_ids, dependencies.last()) {
        source_external_dependency_contains(dependencies.drop_last(), source_ids, target)
    } else {
        source_external_dependency_contains(dependencies.drop_last(), source_ids, target)
            || dependencies.last() == target
    }
}

pub open spec fn external_dependency_contains(
    source_nodes: Seq<crate::ContextNode>,
    source_ids: Seq<ContextNodeId>,
    target: ContextNodeId,
) -> bool
    decreases source_nodes.len(),
{
    if source_nodes.len() == 0 {
        false
    } else {
        external_dependency_contains(source_nodes.drop_last(), source_ids, target)
            || source_external_dependency_contains(
                source_nodes.last().spec_dependencies(), source_ids, target,
            )
    }
}

pub(super) proof fn source_external_dependency_contains_push(
    dependencies: Seq<ContextNodeId>,
    source_ids: Seq<ContextNodeId>,
    dependency: ContextNodeId,
    target: ContextNodeId,
)
    ensures source_external_dependency_contains(
        dependencies.push(dependency), source_ids, target,
    ) == (source_external_dependency_contains(dependencies, source_ids, target)
        || (!contains_id(source_ids, dependency) && dependency == target)),
{
    assert(dependencies.push(dependency).drop_last() =~= dependencies);
    assert(dependencies.push(dependency).last() == dependency);
    reveal_with_fuel(source_external_dependency_contains, 1);
}

pub(super) proof fn external_dependency_contains_push(
    source_nodes: Seq<crate::ContextNode>,
    source_ids: Seq<ContextNodeId>,
    source: crate::ContextNode,
    target: ContextNodeId,
)
    ensures external_dependency_contains(source_nodes.push(source), source_ids, target) ==
        (external_dependency_contains(source_nodes, source_ids, target)
            || source_external_dependency_contains(
                source.spec_dependencies(), source_ids, target,
            )),
{
    assert(source_nodes.push(source).drop_last() =~= source_nodes);
    assert(source_nodes.push(source).last() == source);
    reveal_with_fuel(external_dependency_contains, 1);
}

pub(super) proof fn rewritten_dependency_contains_push(
    dependencies: Seq<ContextNodeId>,
    source_ids: Seq<ContextNodeId>,
    output_id: ContextNodeId,
    dependency: ContextNodeId,
    target: ContextNodeId,
)
    ensures rewritten_dependency_contains(
        dependencies.push(dependency), source_ids, output_id, target,
    ) == (rewritten_dependency_contains(dependencies, source_ids, output_id, target)
        || (if contains_id(source_ids, dependency) { output_id } else { dependency }) == target),
{
    assert(dependencies.push(dependency).drop_last() =~= dependencies);
    assert(dependencies.push(dependency).last() == dependency);
    reveal_with_fuel(rewritten_dependency_contains, 1);
}

pub open spec fn survivor_count(
    nodes: Seq<crate::ContextNode>,
    source_ids: Seq<ContextNodeId>,
) -> nat
    decreases nodes.len(),
{
    if nodes.len() == 0 {
        0
    } else {
        survivor_count(nodes.drop_last(), source_ids)
            + if contains_id(source_ids, nodes.last().spec_id()) { 0nat } else { 1nat }
    }
}

pub(super) proof fn survivor_count_push(
    nodes: Seq<crate::ContextNode>,
    source_ids: Seq<ContextNodeId>,
    node: crate::ContextNode,
)
    ensures survivor_count(nodes.push(node), source_ids) == survivor_count(nodes, source_ids)
        + if contains_id(source_ids, node.spec_id()) { 0nat } else { 1nat },
{
    assert(nodes.push(node).drop_last() =~= nodes);
    assert(nodes.push(node).last() == node);
    reveal_with_fuel(survivor_count, 1);
}

pub open spec fn replacement_node_matches(
    before: &crate::ContextNode,
    after: &crate::ContextNode,
    source_nodes: Seq<crate::ContextNode>,
    source_ids: Seq<ContextNodeId>,
) -> bool {
    &&& crate::ContextNode::dependencies_replaced(before, after, after.spec_dependencies())
    &&& forall |id: ContextNodeId| contains_id(after.spec_dependencies(), id) ==
        external_dependency_contains(source_nodes, source_ids, id)
}

pub open spec fn surviving_node_matches(
    before: &crate::ContextNode,
    after: &crate::ContextNode,
    source_ids: Seq<ContextNodeId>,
    output_id: ContextNodeId,
) -> bool {
    &&& !contains_id(source_ids, before.spec_id())
    &&& crate::ContextNode::dependencies_replaced(before, after, after.spec_dependencies())
    &&& forall |id: ContextNodeId| contains_id(after.spec_dependencies(), id) ==
        rewritten_dependency_contains(before.spec_dependencies(), source_ids, output_id, id)
}

pub open spec fn exact_replacement_graph(
    before: &ContextGraph,
    validated: &ValidatedCompaction,
    after: &ContextGraph,
) -> bool {
    let source_ids = validated.spec_source_ids();
    let output_id = validated.spec_node().spec_id();
    &&& after.spec_limits() == before.spec_limits()
    &&& !contains_id(source_ids, output_id)
    &&& after.spec_nodes().len() == survivor_count(before.spec_nodes(), source_ids) + 1
    &&& exists |index: int| #![trigger after.spec_nodes()[index]] {
        &&& 0 <= index < after.spec_nodes().len()
        &&& replacement_node_matches(
            &validated.spec_node(),
            &after.spec_nodes()[index],
            validated.spec_source_nodes(),
            source_ids,
        )
    }
    &&& forall |after_index: int| #![trigger after.spec_nodes()[after_index]]
        0 <= after_index < after.spec_nodes().len() ==> {
            ||| replacement_node_matches(
                &validated.spec_node(),
                &after.spec_nodes()[after_index],
                validated.spec_source_nodes(),
                source_ids,
            )
            ||| exists |before_index: int| #![trigger before.spec_nodes()[before_index]] {
                &&& 0 <= before_index < before.spec_nodes().len()
                &&& surviving_node_matches(
                    &before.spec_nodes()[before_index],
                    &after.spec_nodes()[after_index],
                    source_ids,
                    output_id,
                )
            }
        }
}

} // verus!
