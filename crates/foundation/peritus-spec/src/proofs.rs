//! Formal gate-order model and acyclicity theorems.

use vstd::prelude::*;

verus! {

/// Returns whether a numeric gate identity appears in a proposed execution order.
pub open spec fn gate_node_present(order: Seq<int>, node: int) -> bool {
    exists |position: int| 0 <= position < order.len() && order[position] == node
}

/// Returns the unique position of a present gate node.
pub open spec fn gate_node_rank(order: Seq<int>, node: int) -> int
    recommends gate_node_present(order, node),
{
    choose |position: int| 0 <= position < order.len() && order[position] == node
}

/// A complete dependency matrix and execution order form an acyclicity certificate.
///
/// Numeric node `n` corresponds to canonical gate definition `n`. Every node occurs exactly once,
/// every dependency names a declared node, and the dependency's rank is strictly smaller than the
/// dependent gate's rank.
pub open spec fn valid_gate_execution_order(
    dependencies: Seq<Seq<int>>,
    order: Seq<int>,
) -> bool {
    &&& order.len() == dependencies.len()
    &&& forall |position: int| 0 <= position < order.len() ==>
        0 <= #[trigger] order[position] < dependencies.len()
    &&& forall |left: int, right: int|
        0 <= left < order.len() && 0 <= right < order.len() && left != right ==>
            order[left] != order[right]
    &&& forall |node: int| 0 <= node < dependencies.len() ==> gate_node_present(order, node)
    &&& forall |gate: int, dependency_position: int|
        0 <= gate < dependencies.len()
            && 0 <= dependency_position < dependencies[gate].len() ==>
        {
            let dependency = #[trigger] dependencies[gate][dependency_position];
            &&& 0 <= dependency < dependencies.len()
            &&& gate_node_rank(order, dependency) < gate_node_rank(order, gate)
        }
}

/// Every declared dependency is scheduled strictly before its dependent gate.
pub proof fn declared_dependency_precedes_gate(
    dependencies: Seq<Seq<int>>,
    order: Seq<int>,
    gate: int,
    dependency_position: int,
)
    requires
        valid_gate_execution_order(dependencies, order),
        0 <= gate < dependencies.len(),
        0 <= dependency_position < dependencies[gate].len(),
    ensures
        gate_node_rank(order, dependencies[gate][dependency_position])
            < gate_node_rank(order, gate),
{
}

/// A valid execution order forbids a self-dependency edge.
pub proof fn valid_order_has_no_self_dependency(
    dependencies: Seq<Seq<int>>,
    order: Seq<int>,
    gate: int,
    dependency_position: int,
)
    requires
        valid_gate_execution_order(dependencies, order),
        0 <= gate < dependencies.len(),
        0 <= dependency_position < dependencies[gate].len(),
    ensures dependencies[gate][dependency_position] != gate,
{
}

/// A valid execution order forbids two gates from depending on each other.
pub proof fn valid_order_has_no_two_node_cycle(
    dependencies: Seq<Seq<int>>,
    order: Seq<int>,
    first: int,
    first_dependency_position: int,
    second_dependency_position: int,
)
    requires
        valid_gate_execution_order(dependencies, order),
        0 <= first < dependencies.len(),
        0 <= first_dependency_position < dependencies[first].len(),
        dependencies[first][first_dependency_position] >= 0,
        dependencies[first][first_dependency_position] < dependencies.len(),
        0 <= second_dependency_position
            < dependencies[dependencies[first][first_dependency_position]].len(),
    ensures
        dependencies[dependencies[first][first_dependency_position]][second_dependency_position]
            != first,
{
}

} // verus!
