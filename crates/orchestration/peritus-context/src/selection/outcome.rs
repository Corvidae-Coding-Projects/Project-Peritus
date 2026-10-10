//! Independent logical success relation and executable required-node check.

use crate::{ContextError, ContextErrorKind, ContextGraph, ContextPlan, RequirementMode};

pub(super) const fn selection_failure_is_typed_exec(error: &ContextError) -> bool {
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
pub(super) fn required_nodes_are_selected(graph: &ContextGraph, plan: &ContextPlan) -> bool {
    let nodes = graph.nodes();
    let mut index = 0;
    while index < nodes.len() {
        match nodes[index].requirement() {
            RequirementMode::Required => {
                let node_id = nodes[index].id();
                if !plan.contains(node_id) {
                    return false;
                }
            }
            RequirementMode::DependencyRequired | RequirementMode::Optional => {}
        }
        index += 1;
    }
    true
}
