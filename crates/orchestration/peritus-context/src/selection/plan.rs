//! Transactional required-first and optional context planning.

use super::SelectionPolicy;
use super::closure::{admit_closure, closure_delta, dependency_closure, first_hidden, is_visible};
use super::ordering::{ranked_optional_roots, sort_for_render};
use super::outcome::required_nodes_are_selected;
#[cfg(verus_only)]
use super::outcome::{certified_selection_outcome, selection_success_matches};
use crate::{
    ContextError, ContextErrorKind, ContextGraph, ContextPlan, ContextPlanId, OmissionReason,
    OmittedContext, RequirementMode, SelectedContext, SelectionReason,
};
use vstd::prelude::*;

verus! {

/// Selects one complete deterministic plan or returns its exact first typed failure.
///
/// # Errors
///
/// Returns a typed error when required content is hidden or cannot fit, or checked arithmetic
/// overflows. Optional failures are represented as atomic omission records in the successful plan.
/// A certificate mismatch reports an internal construction fault rather than exposing an
/// unverified result.
pub fn select_context(
    graph: &ContextGraph,
    policy: &SelectionPolicy,
    plan_id: ContextPlanId,
) -> (result: Result<ContextPlan, ContextError>)
    ensures
        certified_selection_outcome(graph, &result),
        match result {
            Ok(plan) => selection_success_matches(graph, policy, plan_id, &plan),
            Err(_) => true,
        },
{
    let candidate = construct_context_candidate(graph, policy, plan_id);
    if super::certificate::selection_result_is_exact(graph, policy, plan_id, &candidate) {
        candidate
    } else {
        Err(ContextError::plain(ContextErrorKind::SelectionCertificateMismatch))
    }
}

#[allow(clippy::too_many_lines, reason = "required and optional phases share one atomic transaction")]
fn construct_context_candidate(
    graph: &ContextGraph,
    policy: &SelectionPolicy,
    plan_id: ContextPlanId,
) -> (result: Result<ContextPlan, ContextError>)
    ensures match result {
        Ok(plan) => selection_success_matches(graph, policy, plan_id, &plan),
        Err(_) => true,
    },
{
    proof { use_type_invariant(policy); }
    let graph_nodes = graph.nodes();
    let graph_len = graph_nodes.len();
    let mut selected = vec![false; graph_len];
    let mut reasons = vec![None; graph_len];
    let mut used_tokens = 0u64;
    let mut used_bytes = 0usize;
    let mut used_nodes = 0usize;

    let mut index = 0;
    while index < graph_len
        invariant
            index <= graph_len,
            graph_len == graph_nodes@.len(),
            selected.len() == graph_len,
            reasons.len() == graph_len,
            used_tokens as int <= policy.spec_token_budget().spec_usable_input(),
            used_nodes as nat <= policy.spec_max_selected_nodes(),
            used_bytes as nat <= policy.spec_max_selected_bytes(),
        decreases graph_len - index,
    {
        let node = &graph_nodes[index];
        if node.requirement() == RequirementMode::Required {
            if !is_visible(node, policy.role_profile()) {
                return Err(ContextError::node(ContextErrorKind::HiddenRequiredNode, node.id()));
            }
            let closure = dependency_closure(graph, index)?;
            if let Some(hidden) = first_hidden(graph, closure.as_slice(), policy.role_profile()) {
                if hidden >= graph_len {
                    return Err(ContextError::plain(ContextErrorKind::PlanNodeMissing));
                }
                return Err(ContextError::nodes(
                    ContextErrorKind::HiddenRequiredDependency,
                    node.id(),
                    graph_nodes[hidden].id(),
                ));
            }
            let delta = closure_delta(graph, closure.as_slice(), selected.as_slice())?;
            let Some(next_tokens) = used_tokens.checked_add(delta.tokens) else {
                return Err(ContextError::node(ContextErrorKind::ArithmeticOverflow, node.id()));
            };
            if next_tokens > policy.token_budget().usable_input() {
                return Err(ContextError::node_numbers(
                    ContextErrorKind::RequiredTokenBudgetExceeded,
                    node.id(),
                    policy.token_budget().usable_input(),
                    next_tokens,
                ));
            }
            let Some(next_nodes) = used_nodes.checked_add(delta.nodes) else {
                return Err(ContextError::node(ContextErrorKind::ArithmeticOverflow, node.id()));
            };
            if next_nodes > policy.max_selected_nodes() {
                return Err(ContextError::node_numbers(
                    ContextErrorKind::RequiredNodeLimitExceeded,
                    node.id(),
                    policy.max_selected_nodes() as u64,
                    next_nodes as u64,
                ));
            }
            let Some(next_bytes) = used_bytes.checked_add(delta.bytes) else {
                return Err(ContextError::node(ContextErrorKind::ArithmeticOverflow, node.id()));
            };
            if next_bytes > policy.max_selected_bytes() {
                return Err(ContextError::node_numbers(
                    ContextErrorKind::RequiredByteLimitExceeded,
                    node.id(),
                    policy.max_selected_bytes() as u64,
                    next_bytes as u64,
                ));
            }
            admit_closure(
                closure.as_slice(),
                index,
                &mut selected,
                &mut reasons,
                SelectionReason::RequiredRoot,
                SelectionReason::RequiredDependency,
            )?;
            reasons[index] = Some(SelectionReason::RequiredRoot);
            used_tokens = next_tokens;
            used_nodes = next_nodes;
            used_bytes = next_bytes;
        }
        index += 1;
    }

    let ranked = ranked_optional_roots(graph, selected.as_slice(), policy.role_profile());
    let mut omitted = Vec::new();
    let mut rank_index = 0;
    while rank_index < ranked.len()
        invariant
            rank_index <= ranked.len(),
            selected.len() == graph_len,
            reasons.len() == graph_len,
            graph_len == graph_nodes@.len(),
            used_tokens as int <= policy.spec_token_budget().spec_usable_input(),
            used_nodes as nat <= policy.spec_max_selected_nodes(),
            used_bytes as nat <= policy.spec_max_selected_bytes(),
        decreases ranked.len() - rank_index,
    {
        let root = ranked[rank_index];
        if root >= graph_len {
            return Err(ContextError::plain(ContextErrorKind::PlanNodeMissing));
        }
        if selected[root] {
            rank_index += 1;
            continue;
        }
        let closure = dependency_closure(graph, root)?;
        if let Some(hidden) = first_hidden(graph, closure.as_slice(), policy.role_profile()) {
            if hidden >= graph_len {
                return Err(ContextError::plain(ContextErrorKind::PlanNodeMissing));
            }
            omitted.push(OmittedContext::new(
                graph_nodes[root].id(),
                OmissionReason::HiddenDependency,
                Some(graph_nodes[hidden].id()),
                0,
            ));
            rank_index += 1;
            continue;
        }
        let delta = closure_delta(graph, closure.as_slice(), selected.as_slice())?;
        let Some(next_tokens) = used_tokens.checked_add(delta.tokens) else {
            return Err(ContextError::node(
                ContextErrorKind::ArithmeticOverflow,
                graph_nodes[root].id(),
            ));
        };
        let Some(next_nodes) = used_nodes.checked_add(delta.nodes) else {
            return Err(ContextError::node(
                ContextErrorKind::ArithmeticOverflow,
                graph_nodes[root].id(),
            ));
        };
        let Some(next_bytes) = used_bytes.checked_add(delta.bytes) else {
            return Err(ContextError::node(
                ContextErrorKind::ArithmeticOverflow,
                graph_nodes[root].id(),
            ));
        };

        let omission = if next_tokens > policy.token_budget().usable_input() {
            Some(OmissionReason::TokenBudget)
        } else if next_nodes > policy.max_selected_nodes() {
            Some(OmissionReason::NodeLimit)
        } else if next_bytes > policy.max_selected_bytes() {
            Some(OmissionReason::ByteLimit)
        } else {
            None
        };
        if let Some(reason) = omission {
            omitted.push(OmittedContext::new(
                graph_nodes[root].id(),
                reason,
                None,
                delta.tokens,
            ));
        } else {
            admit_closure(
                closure.as_slice(),
                root,
                &mut selected,
                &mut reasons,
                SelectionReason::OptionalRoot,
                SelectionReason::OptionalDependency,
            )?;
            used_tokens = next_tokens;
            used_nodes = next_nodes;
            used_bytes = next_bytes;
        }
        rank_index += 1;
    }

    let mut selected_entries = Vec::with_capacity(used_nodes);
    index = 0;
    while index < graph_len
        invariant
            index <= graph_len,
            selected.len() == graph_len,
            reasons.len() == graph_len,
            graph_len == graph_nodes@.len(),
            selected_entries@.len() <= index,
            used_tokens as int <= policy.spec_token_budget().spec_usable_input(),
            used_bytes as nat <= policy.spec_max_selected_bytes(),
        decreases graph_len - index,
    {
        if selected[index] {
            let Some(reason) = reasons[index] else {
                return Err(ContextError::node(
                    ContextErrorKind::PlanNodeMissing,
                    graph_nodes[index].id(),
                ));
            };
            selected_entries.push(SelectedContext::new(graph_nodes[index].id(), reason));
        }
        index += 1;
    }
    sort_for_render(graph, &mut selected_entries);
    let accounting = policy.token_budget().accounting(used_tokens)?;
    let plan = ContextPlan::new(
        plan_id,
        policy.role_profile().clone(),
        selected_entries,
        omitted,
        accounting,
        used_bytes,
    );
    let plan_is_exact = crate::plan_is_exact(graph, &plan);
    let required_complete = required_nodes_are_selected(graph, &plan);
    if plan.selected().len() > policy.max_selected_nodes()
        || !plan_is_exact
        || !required_complete
    {
        return Err(ContextError::plain(ContextErrorKind::PlanNodeMissing));
    }
    proof {
        reveal(selection_success_matches);
        assert(selection_success_matches(graph, policy, plan_id, &plan));
    }
    Ok(plan)
}

} // verus!
