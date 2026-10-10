//! Complete dependency-closure formation, validation, and admission.

use crate::{ContextError, ContextErrorKind, ContextGraph, SelectionReason};
use peritus_role::RoleProfile;

#[derive(Clone, Copy)]
pub(super) struct ClosureDelta {
    pub(super) tokens: u64,
    pub(super) bytes: usize,
    pub(super) nodes: usize,
}
pub(super) fn is_visible(node: &crate::ContextNode, role: &RoleProfile) -> bool {
    node.visibility().contains(role.actor_role())
        && role.context().visible().contains(node.context_class())
}
pub(super) fn dependency_closure(
    graph: &ContextGraph,
    root: usize,
) -> Result<Vec<usize>, ContextError> {
    let graph_nodes = graph.nodes();
    let graph_len = graph_nodes.len();
    if root >= graph_len {
        return Err(ContextError::plain(ContextErrorKind::PlanNodeMissing));
    }
    let mut included = vec![false; graph_len];
    included[root] = true;
    let mut pass = 0;
    while pass < graph_len {
        let mut node_index = 0;
        while node_index < graph_len {
            if included[node_index] {
                let dependencies = graph_nodes[node_index].dependencies();
                let mut dependency_index = 0;
                while dependency_index < dependencies.len() {
                    let Some(target) = graph.index_of(dependencies[dependency_index]) else {
                        return Err(ContextError::nodes(
                            ContextErrorKind::MissingDependency,
                            graph_nodes[node_index].id(),
                            dependencies[dependency_index],
                        ));
                    };
                    if target >= graph_len || target >= included.len() {
                        return Err(ContextError::plain(ContextErrorKind::PlanNodeMissing));
                    }
                    included[target] = true;
                    dependency_index += 1;
                }
            }
            node_index += 1;
        }
        pass += 1;
    }
    let mut closure = Vec::new();
    let mut index = 0;
    while index < included.len() {
        if included[index] {
            closure.push(index);
        }
        index += 1;
    }
    Ok(closure)
}
pub(super) fn first_hidden(
    graph: &ContextGraph,
    closure: &[usize],
    role: &RoleProfile,
) -> Option<usize> {
    let graph_nodes = graph.nodes();
    let mut index = 0;
    while index < closure.len() {
        let node_index = closure[index];
        if node_index >= graph_nodes.len() || !is_visible(&graph_nodes[node_index], role) {
            return Some(node_index);
        }
        index += 1;
    }
    None
}
pub(super) fn closure_delta(
    graph: &ContextGraph,
    closure: &[usize],
    selected: &[bool],
) -> Result<ClosureDelta, ContextError> {
    let graph_nodes = graph.nodes();
    let mut delta = ClosureDelta { tokens: 0, bytes: 0, nodes: 0 };
    let mut index = 0;
    while index < closure.len() {
        let node_index = closure[index];
        if node_index >= graph_nodes.len() || node_index >= selected.len() {
            return Err(ContextError::plain(ContextErrorKind::PlanNodeMissing));
        }
        if !selected[node_index] {
            let Some(next_tokens) =
                delta.tokens.checked_add(graph_nodes[node_index].token_estimate())
            else {
                return Err(ContextError::node(
                    ContextErrorKind::ArithmeticOverflow,
                    graph_nodes[node_index].id(),
                ));
            };
            let Some(next_bytes) = delta.bytes.checked_add(graph_nodes[node_index].content().len())
            else {
                return Err(ContextError::node(
                    ContextErrorKind::ArithmeticOverflow,
                    graph_nodes[node_index].id(),
                ));
            };
            let Some(next_nodes) = delta.nodes.checked_add(1) else {
                return Err(ContextError::node(
                    ContextErrorKind::ArithmeticOverflow,
                    graph_nodes[node_index].id(),
                ));
            };
            delta.tokens = next_tokens;
            delta.bytes = next_bytes;
            delta.nodes = next_nodes;
        }
        index += 1;
    }
    Ok(delta)
}
pub(super) fn admit_closure(
    closure: &[usize],
    root: usize,
    selected: &mut [bool],
    reasons: &mut [Option<SelectionReason>],
    root_reason: SelectionReason,
    dependency_reason: SelectionReason,
) -> Result<(), ContextError> {
    let mut index = 0;
    while index < closure.len() {
        let node_index = closure[index];
        if node_index >= selected.len() || node_index >= reasons.len() {
            return Err(ContextError::plain(ContextErrorKind::PlanNodeMissing));
        }
        if !selected[node_index] {
            selected[node_index] = true;
            reasons[node_index] =
                Some(if node_index == root { root_reason } else { dependency_reason });
        }
        index += 1;
    }
    Ok(())
}
