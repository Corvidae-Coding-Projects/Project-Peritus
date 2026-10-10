//! Executable refinement of the stored gate-graph validity certificate.

use crate::GateDefinition;
use peritus_types::GateId;

mod canonical;
const fn gate_id_matches(left: GateId, right: GateId) -> bool {
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
fn gate_position_from_exec(order: &[GateId], target: GateId, index: usize) -> Option<usize> {
    if index >= order.len() {
        None
    } else if gate_id_matches(order[index], target) {
        Some(index)
    } else {
        gate_position_from_exec(order, target, index + 1)
    }
}
fn definition_position_from_exec(
    definitions: &[GateDefinition],
    target: GateId,
    index: usize,
) -> Option<usize> {
    if index >= definitions.len() {
        None
    } else if gate_id_matches(definitions[index].id(), target) {
        Some(index)
    } else {
        definition_position_from_exec(definitions, target, index + 1)
    }
}
fn order_unique_from_exec(order: &[GateId], index: usize) -> bool {
    if index >= order.len() {
        true
    } else if gate_position_from_exec(order, order[index], index + 1).is_some() {
        false
    } else {
        order_unique_from_exec(order, index + 1)
    }
}
fn definitions_unique_from_exec(definitions: &[GateDefinition], index: usize) -> bool {
    if index >= definitions.len() {
        true
    } else if definition_position_from_exec(definitions, definitions[index].id(), index + 1)
        .is_some()
    {
        false
    } else {
        definitions_unique_from_exec(definitions, index + 1)
    }
}
fn order_declared_from_exec(
    definitions: &[GateDefinition],
    order: &[GateId],
    index: usize,
) -> bool {
    if index >= order.len() {
        true
    } else if definition_position_from_exec(definitions, order[index], 0).is_none() {
        false
    } else {
        order_declared_from_exec(definitions, order, index + 1)
    }
}
fn dependencies_precede_from_exec(
    dependencies: &[GateId],
    order: &[GateId],
    gate_position: usize,
    index: usize,
) -> bool {
    if index >= dependencies.len() {
        true
    } else {
        match gate_position_from_exec(order, dependencies[index], 0) {
            Some(dependency_position) if dependency_position < gate_position => {
                dependencies_precede_from_exec(dependencies, order, gate_position, index + 1)
            }
            _ => false,
        }
    }
}
fn definitions_respect_order_from_exec(
    definitions: &[GateDefinition],
    order: &[GateId],
    index: usize,
) -> bool {
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
#[allow(
    clippy::redundant_pub_crate,
    reason = "Verus requires crate visibility for this cross-module executable refinement"
)]
pub(crate) fn execution_order_is_valid(definitions: &[GateDefinition], order: &[GateId]) -> bool {
    !definitions.is_empty()
        && definitions.len() == order.len()
        && definitions_unique_from_exec(definitions, 0)
        && order_unique_from_exec(order, 0)
        && order_declared_from_exec(definitions, order, 0)
        && definitions_respect_order_from_exec(definitions, order, 0)
        && canonical::canonical_execution_from_exec(definitions, order, 0)
}
