//! Production validation and exact lookup for canonical context graphs.

use crate::{ContextError, ContextErrorKind, ContextNode, ContextNodeId};
use core::cmp::Ordering;

pub(super) fn validate_node_order(nodes: &[ContextNode]) -> Result<(), ContextError> {
    let mut index = 1;
    while index < nodes.len() {
        match nodes[index - 1].id().canonical_order(&nodes[index].id()) {
            Ordering::Equal => {
                return Err(ContextError::node(
                    ContextErrorKind::DuplicateValue,
                    nodes[index].id(),
                ));
            }
            Ordering::Greater => {
                return Err(ContextError::node(
                    ContextErrorKind::NonCanonicalOrder,
                    nodes[index].id(),
                ));
            }
            Ordering::Less => {}
        }
        index += 1;
    }
    Ok(())
}
pub(super) fn find_node_index(nodes: &[ContextNode], id: ContextNodeId) -> Option<usize> {
    let mut index = 0;
    while index < nodes.len() {
        if nodes[index].id().matches(&id) {
            return Some(index);
        }
        index += 1;
    }
    None
}
fn validate_node_dependencies(
    nodes: &[ContextNode],
    owner: ContextNodeId,
    dependencies: &[ContextNodeId],
) -> Result<(), ContextError> {
    let mut index = 0;
    while index < dependencies.len() {
        if find_node_index(nodes, dependencies[index]).is_none() {
            return Err(ContextError::nodes(
                ContextErrorKind::MissingDependency,
                owner,
                dependencies[index],
            ));
        }
        index += 1;
    }
    Ok(())
}
pub(super) fn validate_dependencies(nodes: &[ContextNode]) -> Result<(), ContextError> {
    let mut index = 0;
    while index < nodes.len() {
        validate_node_dependencies(nodes, nodes[index].id(), nodes[index].dependencies())?;
        index += 1;
    }
    Ok(())
}
