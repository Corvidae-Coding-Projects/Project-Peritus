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

/// Returns whether the complete plan is an exact, internally consistent view of the graph.
///
/// This independently rechecks source existence, uniqueness, dependency closure, visibility,
/// render order, omission ownership, and token/byte accounting before a planner result escapes.
#[must_use]
pub fn plan_is_exact(graph: &ContextGraph, plan: &ContextPlan) -> bool {
    if !plan_is_visible(graph, plan)
        || !plan_dependencies_complete(graph, plan)
        || !token_accounting_is_bounded(plan.accounting())
    {
        return false;
    }
    let selected = plan.selected();
    let mut tokens = 0_u64;
    let mut bytes = 0_usize;
    let mut index = 0;
    while index < selected.len()
        invariant index <= selected.len(),
        decreases selected.len() - index,
    {
        let Some(node) = graph.node(selected[index].node_id()) else { return false };
        let mut prior = 0;
        while prior < index
            invariant prior <= index, index < selected.len(),
            decreases index - prior,
        {
            if selected[prior].node_id() == selected[index].node_id() {
                return false;
            }
            prior += 1;
        }
        if index > 0 {
            let Some(previous) = graph.node(selected[index - 1].node_id()) else { return false };
            if !crate::precedence::render_precedes(previous, node) {
                return false;
            }
        }
        let Some(next_tokens) = tokens.checked_add(node.token_estimate()) else { return false };
        let Some(next_bytes) = bytes.checked_add(node.content().len()) else { return false };
        tokens = next_tokens;
        bytes = next_bytes;
        index += 1;
    }
    if tokens != plan.accounting().used_input() || bytes != plan.selected_bytes() {
        return false;
    }
    let omitted = plan.omitted();
    index = 0;
    while index < omitted.len()
        invariant index <= omitted.len(),
        decreases omitted.len() - index,
    {
        let Some(node) = graph.node(omitted[index].node_id()) else { return false };
        if node.requirement() != crate::RequirementMode::Optional || plan.contains(node.id()) {
            return false;
        }
        let mut prior = 0;
        while prior < index
            invariant prior <= index, index < omitted.len(),
            decreases index - prior,
        {
            if omitted[prior].node_id() == omitted[index].node_id() {
                return false;
            }
            prior += 1;
        }
        index += 1;
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
