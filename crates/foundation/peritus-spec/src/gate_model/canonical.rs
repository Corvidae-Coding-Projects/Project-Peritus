//! Canonical Kahn-order certificate over the stored graph.

use super::{dependencies_precede_from_exec, gate_id_matches, gate_position_from_exec};
use crate::GateDefinition;
use peritus_types::GateId;

fn definition_is_eligible_at_exec(
    definition: &GateDefinition,
    order: &[GateId],
    position: usize,
) -> bool {
    #[allow(clippy::option_if_let_else, reason = "explicit branches mirror the Verus relation")]
    let not_scheduled = match gate_position_from_exec(order, definition.id(), 0) {
        Some(existing) => existing >= position,
        None => true,
    };
    not_scheduled && dependencies_precede_from_exec(definition.dependencies(), order, position, 0)
}
fn first_eligible_from_exec(
    definitions: &[GateDefinition],
    order: &[GateId],
    position: usize,
    index: usize,
) -> Option<GateId> {
    if index >= definitions.len() {
        None
    } else if definition_is_eligible_at_exec(&definitions[index], order, position) {
        Some(definitions[index].id())
    } else {
        first_eligible_from_exec(definitions, order, position, index + 1)
    }
}
pub(super) fn canonical_execution_from_exec(
    definitions: &[GateDefinition],
    order: &[GateId],
    position: usize,
) -> bool {
    if position >= order.len() {
        true
    } else {
        match first_eligible_from_exec(definitions, order, position, 0) {
            Some(selected) if gate_id_matches(order[position], selected) => {
                canonical_execution_from_exec(definitions, order, position + 1)
            }
            _ => false,
        }
    }
}
