//! Canonical Kahn-order certificate over the stored graph.

#[cfg(verus_only)]
use super::{dependencies_precede_from, gate_ids_match, gate_position_from};
use super::{dependencies_precede_from_exec, gate_id_matches, gate_position_from_exec};
use crate::GateDefinition;
use peritus_types::GateId;
use vstd::prelude::*;

verus! {

pub(super) open spec fn definition_is_eligible_at(
    definition: &GateDefinition,
    order: Seq<GateId>,
    position: nat,
) -> bool {
    let not_scheduled = match gate_position_from(order, definition.spec_id(), 0) {
        Some(existing) => existing >= position,
        None => true,
    };
    not_scheduled
        && dependencies_precede_from(
            definition.spec_dependencies(),
            order,
            position as int,
            0,
        )
}

fn definition_is_eligible_at_exec(
    definition: &GateDefinition,
    order: &[GateId],
    position: usize,
) -> (eligible: bool)
    ensures eligible == definition_is_eligible_at(definition, order@, position as nat),
{
    #[allow(clippy::option_if_let_else, reason = "explicit branches mirror the Verus relation")]
    let not_scheduled = match gate_position_from_exec(order, definition.id(), 0) {
        Some(existing) => existing >= position,
        None => true,
    };
    not_scheduled
        && dependencies_precede_from_exec(
            definition.dependencies(),
            order,
            position,
            0,
        )
}

pub(super) open spec fn first_eligible_from(
    definitions: Seq<GateDefinition>,
    order: Seq<GateId>,
    position: nat,
    index: nat,
) -> Option<GateId>
    decreases definitions.len() - index,
{
    if index >= definitions.len() {
        None
    } else if definition_is_eligible_at(&definitions[index as int], order, position) {
        Some(definitions[index as int].spec_id())
    } else {
        first_eligible_from(definitions, order, position, index + 1)
    }
}

fn first_eligible_from_exec(
    definitions: &[GateDefinition],
    order: &[GateId],
    position: usize,
    index: usize,
) -> (result: Option<GateId>)
    requires index <= definitions.len(),
    ensures
        match (result, first_eligible_from(
            definitions@,
            order@,
            position as nat,
            index as nat,
        )) {
            (Some(actual), Some(expected)) => gate_ids_match(actual, expected),
            (None, None) => true,
            _ => false,
        },
    decreases definitions.len() - index,
{
    if index >= definitions.len() {
        None
    } else if definition_is_eligible_at_exec(&definitions[index], order, position) {
        Some(definitions[index].id())
    } else {
        first_eligible_from_exec(definitions, order, position, index + 1)
    }
}

pub(super) open spec fn canonical_execution_from(
    definitions: Seq<GateDefinition>,
    order: Seq<GateId>,
    position: nat,
) -> bool
    decreases order.len() - position,
{
    if position >= order.len() {
        true
    } else {
        match first_eligible_from(definitions, order, position, 0) {
            Some(selected) => {
                gate_ids_match(order[position as int], selected)
                    && canonical_execution_from(definitions, order, position + 1)
            }
            None => false,
        }
    }
}

pub(super) fn canonical_execution_from_exec(
    definitions: &[GateDefinition],
    order: &[GateId],
    position: usize,
) -> (canonical: bool)
    requires position <= order.len(),
    ensures canonical == canonical_execution_from(definitions@, order@, position as nat),
    decreases order.len() - position,
{
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

} // verus!
