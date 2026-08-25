//! Executable invariant checks used by ordinary callers and focused proof roots.

use crate::{ContextGraph, ContextPlan, TokenAccounting};
use vstd::prelude::*;

verus! {

/// Returns whether every selected node remains visible to the plan's frozen role profile.
#[must_use]
pub fn plan_is_visible(graph: &ContextGraph, plan: &ContextPlan) -> bool {
    let selected = plan.selected();
    let selected_len = selected.len();
    let mut index = 0;
    while index < selected_len
        invariant
            index <= selected_len,
            selected_len == selected@.len(),
        decreases selected_len - index,
    {
        let Some(node) = graph.node(selected[index].node_id()) else { return false };
        if !node.visibility().contains(plan.role_profile().actor_role())
            || !plan.role_profile().context().visible().contains(node.context_class())
        {
            return false;
        }
        index += 1;
    }
    true
}

/// Returns whether every dependency of every selected node is also selected.
#[must_use]
pub fn plan_dependencies_complete(graph: &ContextGraph, plan: &ContextPlan) -> bool {
    let selected = plan.selected();
    let selected_len = selected.len();
    let mut selected_index = 0;
    while selected_index < selected_len
        invariant
            selected_index <= selected_len,
            selected_len == selected@.len(),
        decreases selected_len - selected_index,
    {
        let Some(node) = graph.node(selected[selected_index].node_id()) else {
            return false;
        };
        let dependencies = node.dependencies();
        let dependencies_len = dependencies.len();
        let mut dependency_index = 0;
        while dependency_index < dependencies_len
            invariant
                dependency_index <= dependencies_len,
                dependencies_len == dependencies@.len(),
            decreases dependencies_len - dependency_index,
        {
            if !plan.contains(dependencies[dependency_index]) {
                return false;
            }
            dependency_index += 1;
        }
        selected_index += 1;
    }
    true
}

/// Returns whether every accounting equality and context-window bound holds.
#[must_use]
pub const fn token_accounting_is_bounded(accounting: TokenAccounting) -> (result: bool)
    ensures result == accounting.spec_is_bounded(),
{
    if accounting.reserved_output() > accounting.context_window() {
        return false;
    }
    let after_output = accounting.context_window() - accounting.reserved_output();
    if accounting.reserved_protocol_overhead() > after_output {
        return false;
    }
    let after_overhead = after_output - accounting.reserved_protocol_overhead();
    if accounting.used_input() > after_overhead {
        return false;
    }
    if accounting.used_input() > accounting.usable_input() {
        return false;
    }
    accounting.remaining_input() == accounting.usable_input() - accounting.used_input()
}

} // verus!
