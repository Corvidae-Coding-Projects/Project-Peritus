//! Executable refinement of the stored gate-graph validity certificate.

use crate::GateDefinition;
use peritus_types::GateId;
use vstd::prelude::*;

verus! {

mod canonical;

pub(super) open spec fn gate_ids_match(left: GateId, right: GateId) -> bool {
    let left = left.spec_bytes();
    let right = right.spec_bytes();
    left[0] == right[0]
        && left[1] == right[1]
        && left[2] == right[2]
        && left[3] == right[3]
        && left[4] == right[4]
        && left[5] == right[5]
        && left[6] == right[6]
        && left[7] == right[7]
        && left[8] == right[8]
        && left[9] == right[9]
        && left[10] == right[10]
        && left[11] == right[11]
        && left[12] == right[12]
        && left[13] == right[13]
        && left[14] == right[14]
        && left[15] == right[15]
}

const fn gate_id_matches(left: GateId, right: GateId) -> (matches: bool)
    ensures matches == gate_ids_match(left, right),
{
    let left = left.as_bytes();
    let right = right.as_bytes();
    left[0] == right[0]
        && left[1] == right[1]
        && left[2] == right[2]
        && left[3] == right[3]
        && left[4] == right[4]
        && left[5] == right[5]
        && left[6] == right[6]
        && left[7] == right[7]
        && left[8] == right[8]
        && left[9] == right[9]
        && left[10] == right[10]
        && left[11] == right[11]
        && left[12] == right[12]
        && left[13] == right[13]
        && left[14] == right[14]
        && left[15] == right[15]
}

pub(super) open spec fn gate_position_from(
    order: Seq<GateId>,
    target: GateId,
    index: nat,
) -> Option<int>
    decreases order.len() - index,
{
    if index >= order.len() {
        None
    } else if gate_ids_match(order[index as int], target) {
        Some(index as int)
    } else {
        gate_position_from(order, target, index + 1)
    }
}

fn gate_position_from_exec(
    order: &[GateId],
    target: GateId,
    index: usize,
) -> (result: Option<usize>)
    requires index <= order.len(),
    ensures
        match result {
            Some(position) => gate_position_from(order@, target, index as nat)
                == Some(position as int),
            None => gate_position_from(order@, target, index as nat).is_none(),
        },
    decreases order.len() - index,
{
    if index >= order.len() {
        None
    } else if gate_id_matches(order[index], target) {
        Some(index)
    } else {
        gate_position_from_exec(order, target, index + 1)
    }
}

pub(super) open spec fn definition_position_from(
    definitions: Seq<GateDefinition>,
    target: GateId,
    index: nat,
) -> Option<int>
    decreases definitions.len() - index,
{
    if index >= definitions.len() {
        None
    } else if gate_ids_match(definitions[index as int].spec_id(), target) {
        Some(index as int)
    } else {
        definition_position_from(definitions, target, index + 1)
    }
}

fn definition_position_from_exec(
    definitions: &[GateDefinition],
    target: GateId,
    index: usize,
) -> (result: Option<usize>)
    requires index <= definitions.len(),
    ensures
        match result {
            Some(position) => definition_position_from(definitions@, target, index as nat)
                == Some(position as int),
            None => definition_position_from(definitions@, target, index as nat).is_none(),
        },
    decreases definitions.len() - index,
{
    if index >= definitions.len() {
        None
    } else if gate_id_matches(definitions[index].id(), target) {
        Some(index)
    } else {
        definition_position_from_exec(definitions, target, index + 1)
    }
}

pub(super) open spec fn order_unique_from(order: Seq<GateId>, index: nat) -> bool
    decreases order.len() - index,
{
    if index >= order.len() {
        true
    } else {
        gate_position_from(order, order[index as int], index + 1).is_none()
            && order_unique_from(order, index + 1)
    }
}

fn order_unique_from_exec(order: &[GateId], index: usize) -> (unique: bool)
    requires index <= order.len(),
    ensures unique == order_unique_from(order@, index as nat),
    decreases order.len() - index,
{
    if index >= order.len() {
        true
    } else if gate_position_from_exec(order, order[index], index + 1).is_some() {
        false
    } else {
        order_unique_from_exec(order, index + 1)
    }
}

pub(super) open spec fn definitions_unique_from(
    definitions: Seq<GateDefinition>,
    index: nat,
) -> bool
    decreases definitions.len() - index,
{
    if index >= definitions.len() {
        true
    } else {
        definition_position_from(
            definitions,
            definitions[index as int].spec_id(),
            index + 1,
        ).is_none() && definitions_unique_from(definitions, index + 1)
    }
}

fn definitions_unique_from_exec(
    definitions: &[GateDefinition],
    index: usize,
) -> (unique: bool)
    requires index <= definitions.len(),
    ensures unique == definitions_unique_from(definitions@, index as nat),
    decreases definitions.len() - index,
{
    if index >= definitions.len() {
        true
    } else if definition_position_from_exec(
        definitions,
        definitions[index].id(),
        index + 1,
    ).is_some() {
        false
    } else {
        definitions_unique_from_exec(definitions, index + 1)
    }
}

pub(super) open spec fn order_declared_from(
    definitions: Seq<GateDefinition>,
    order: Seq<GateId>,
    index: nat,
) -> bool
    decreases order.len() - index,
{
    if index >= order.len() {
        true
    } else {
        definition_position_from(definitions, order[index as int], 0).is_some()
            && order_declared_from(definitions, order, index + 1)
    }
}

fn order_declared_from_exec(
    definitions: &[GateDefinition],
    order: &[GateId],
    index: usize,
) -> (declared: bool)
    requires index <= order.len(),
    ensures declared == order_declared_from(definitions@, order@, index as nat),
    decreases order.len() - index,
{
    if index >= order.len() {
        true
    } else if definition_position_from_exec(definitions, order[index], 0).is_none() {
        false
    } else {
        order_declared_from_exec(definitions, order, index + 1)
    }
}

pub(super) open spec fn dependencies_precede_from(
    dependencies: Seq<GateId>,
    order: Seq<GateId>,
    gate_position: int,
    index: nat,
) -> bool
    decreases dependencies.len() - index,
{
    if index >= dependencies.len() {
        true
    } else {
        match gate_position_from(order, dependencies[index as int], 0) {
            Some(dependency_position) => {
                dependency_position < gate_position
                    && dependencies_precede_from(dependencies, order, gate_position, index + 1)
            }
            None => false,
        }
    }
}

fn dependencies_precede_from_exec(
    dependencies: &[GateId],
    order: &[GateId],
    gate_position: usize,
    index: usize,
) -> (precedes: bool)
    requires index <= dependencies.len(),
    ensures
        precedes == dependencies_precede_from(
            dependencies@,
            order@,
            gate_position as int,
            index as nat,
        ),
    decreases dependencies.len() - index,
{
    if index >= dependencies.len() {
        true
    } else {
        match gate_position_from_exec(order, dependencies[index], 0) {
            Some(dependency_position) if dependency_position < gate_position => {
                dependencies_precede_from_exec(
                    dependencies,
                    order,
                    gate_position,
                    index + 1,
                )
            }
            _ => false,
        }
    }
}

pub(super) open spec fn definitions_respect_order_from(
    definitions: Seq<GateDefinition>,
    order: Seq<GateId>,
    index: nat,
) -> bool
    decreases definitions.len() - index,
{
    if index >= definitions.len() {
        true
    } else {
        match gate_position_from(order, definitions[index as int].spec_id(), 0) {
            Some(gate_position) => {
                dependencies_precede_from(
                    definitions[index as int].spec_dependencies(),
                    order,
                    gate_position,
                    0,
                ) && definitions_respect_order_from(definitions, order, index + 1)
            }
            None => false,
        }
    }
}

fn definitions_respect_order_from_exec(
    definitions: &[GateDefinition],
    order: &[GateId],
    index: usize,
) -> (respects: bool)
    requires index <= definitions.len(),
    ensures respects == definitions_respect_order_from(definitions@, order@, index as nat),
    decreases definitions.len() - index,
{
    if index >= definitions.len() {
        true
    } else {
        #[allow(clippy::option_if_let_else, reason = "explicit branches mirror the Verus relation")]
        match gate_position_from_exec(order, definitions[index].id(), 0) {
            Some(gate_position) => {
                let dependencies = definitions[index].dependencies();
                dependencies_precede_from_exec(dependencies, order, gate_position, 0)
                    && definitions_respect_order_from_exec(definitions, order, index + 1)
            }
            None => false,
        }
    }
}

/// Returns whether stored definitions and execution order are a complete acyclic gate graph.
pub closed spec fn gate_execution_order_is_valid(
    definitions: Seq<GateDefinition>,
    order: Seq<GateId>,
) -> bool {
    definitions.len() > 0
        && definitions.len() == order.len()
        && definitions_unique_from(definitions, 0)
        && order_unique_from(order, 0)
        && order_declared_from(definitions, order, 0)
        && definitions_respect_order_from(definitions, order, 0)
        && canonical::canonical_execution_from(definitions, order, 0)
}

#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for this cross-module executable refinement"
)]
pub(crate) fn execution_order_is_valid(
    definitions: &[GateDefinition],
    order: &[GateId],
) -> (valid: bool)
    ensures valid == gate_execution_order_is_valid(definitions@, order@),
{
    !definitions.is_empty()
        && definitions.len() == order.len()
        && definitions_unique_from_exec(definitions, 0)
        && order_unique_from_exec(order, 0)
        && order_declared_from_exec(definitions, order, 0)
        && definitions_respect_order_from_exec(definitions, order, 0)
        && canonical::canonical_execution_from_exec(definitions, order, 0)
}

/// Extracts the edge-order certificate from a validated stored graph.
pub(super) proof fn valid_graph_has_dependency_order_certificate(
    definitions: Seq<GateDefinition>,
    order: Seq<GateId>,
)
    requires gate_execution_order_is_valid(definitions, order),
    ensures definitions_respect_order_from(definitions, order, 0),
{
}

} // verus!
