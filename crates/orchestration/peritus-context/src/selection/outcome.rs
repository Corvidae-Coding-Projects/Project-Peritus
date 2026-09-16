//! Independent logical success relation and executable required-node check.

#[cfg(verus_only)]
use crate::plan_exact_relation;
#[cfg(verus_only)]
use crate::{ContextPlanId, SelectionPolicy};
use crate::{ContextError, ContextErrorKind, ContextGraph, ContextPlan, RequirementMode};
use vstd::prelude::*;

verus! {

pub open spec fn required_selection_is_complete(
    graph: &ContextGraph,
    plan: &ContextPlan,
) -> bool {
    forall |index: int| #![trigger graph.spec_nodes()[index]]
        0 <= index < graph.spec_nodes().len()
            && graph.spec_nodes()[index].spec_requirement() == RequirementMode::Required ==>
                plan.spec_contains(graph.spec_nodes()[index].spec_id())
}

/// Logical meaning of a complete successful selection outcome.
pub open spec fn selection_success_matches(
    graph: &ContextGraph,
    policy: &SelectionPolicy,
    plan_id: ContextPlanId,
    plan: &ContextPlan,
) -> bool {
    &&& plan.spec_id() == plan_id
    &&& plan.spec_respects_policy(policy)
    &&& plan.spec_selected().len() <= graph.spec_nodes().len()
    &&& plan.spec_selected().len() <= policy.spec_max_selected_nodes()
    &&& plan_exact_relation(graph, plan)
    &&& required_selection_is_complete(graph, plan)
}

/// Exact field shape admitted for a selection failure.
pub open spec fn selection_failure_is_typed(error: &ContextError) -> bool {
    match error.spec_kind() {
        ContextErrorKind::MissingDependency | ContextErrorKind::HiddenRequiredDependency => {
            &&& error.spec_node_id().is_some()
            &&& error.spec_related_id().is_some()
            &&& error.spec_expected().is_none()
            &&& error.spec_actual().is_none()
        }
        ContextErrorKind::ArithmeticOverflow | ContextErrorKind::HiddenRequiredNode => {
            &&& error.spec_node_id().is_some()
            &&& error.spec_related_id().is_none()
            &&& error.spec_expected().is_none()
            &&& error.spec_actual().is_none()
        }
        ContextErrorKind::RequiredTokenBudgetExceeded
        | ContextErrorKind::RequiredNodeLimitExceeded
        | ContextErrorKind::RequiredByteLimitExceeded => {
            &&& error.spec_node_id().is_some()
            &&& error.spec_related_id().is_none()
            &&& error.spec_expected().is_some()
            &&& error.spec_actual().is_some()
        }
        ContextErrorKind::PlanNodeMissing => {
            &&& error.spec_related_id().is_none()
            &&& error.spec_expected().is_none()
            &&& error.spec_actual().is_none()
        }
        ContextErrorKind::SelectionCertificateMismatch => {
            &&& error.spec_node_id().is_none()
            &&& error.spec_related_id().is_none()
            &&& error.spec_expected().is_none()
            &&& error.spec_actual().is_none()
        }
        _ => false,
    }
}

/// Independent logical outcome admitted by the replay certificate.
pub open spec fn certified_selection_outcome(
    graph: &ContextGraph,
    candidate: &Result<ContextPlan, ContextError>,
) -> bool {
    match candidate {
        Ok(plan) => plan_exact_relation(graph, plan)
            && required_selection_is_complete(graph, plan),
        Err(error) => selection_failure_is_typed(error),
    }
}

pub(super) const fn selection_failure_is_typed_exec(error: &ContextError) -> (typed: bool)
    ensures typed == selection_failure_is_typed(error),
{
    let node = error.node_id();
    let related = error.related_id();
    let expected = error.expected();
    let actual = error.actual();
    match error.kind() {
        ContextErrorKind::MissingDependency | ContextErrorKind::HiddenRequiredDependency => {
            node.is_some() && related.is_some() && expected.is_none() && actual.is_none()
        }
        ContextErrorKind::ArithmeticOverflow | ContextErrorKind::HiddenRequiredNode => {
            node.is_some() && related.is_none() && expected.is_none() && actual.is_none()
        }
        ContextErrorKind::RequiredTokenBudgetExceeded
        | ContextErrorKind::RequiredNodeLimitExceeded
        | ContextErrorKind::RequiredByteLimitExceeded => {
            node.is_some() && related.is_none() && expected.is_some() && actual.is_some()
        }
        ContextErrorKind::PlanNodeMissing => {
            related.is_none() && expected.is_none() && actual.is_none()
        }
        ContextErrorKind::SelectionCertificateMismatch => {
            node.is_none() && related.is_none() && expected.is_none() && actual.is_none()
        }
        _ => false,
    }
}

pub(super) fn required_nodes_are_selected(
    graph: &ContextGraph,
    plan: &ContextPlan,
) -> (complete: bool)
    ensures complete == required_selection_is_complete(graph, plan),
{
    let nodes = graph.nodes();
    let mut index = 0;
    while index < nodes.len()
        invariant
            index <= nodes.len(),
            nodes@ == graph.spec_nodes(),
            forall |prior: int| #![trigger nodes@[prior]] 0 <= prior < index
                && nodes@[prior].spec_requirement() == RequirementMode::Required ==>
                    plan.spec_contains(nodes@[prior].spec_id()),
        decreases nodes.len() - index,
    {
        match nodes[index].requirement() {
            RequirementMode::Required => {
            let node_id = nodes[index].id();
            if !plan.contains(node_id) {
                proof {
                    reveal(required_selection_is_complete);
                    assert(nodes@[index as int].spec_requirement()
                        == RequirementMode::Required);
                    assert(!plan.spec_contains(nodes@[index as int].spec_id()));
                    assert(!required_selection_is_complete(graph, plan));
                }
                return false;
            }
            proof {
                assert(plan.spec_contains(nodes@[index as int].spec_id()));
            };
            }
            RequirementMode::DependencyRequired | RequirementMode::Optional => {}
        }
        index += 1;
    }
    proof {
        reveal(required_selection_is_complete);
    }
    true
}

} // verus!
