//! Canonically ordered, bounded context dependency graph.

use crate::{ContextError, ContextErrorKind, ContextLimits, ContextNode, ContextNodeId};

mod cycle;
mod cycle_count_init;
mod cycle_count_model;
mod cycle_count_update;
mod cycle_counts;
mod cycle_edge_model;
mod cycle_model;
mod cycle_proofs;
mod model;
mod validation;

#[doc = " Immutable canonical directed acyclic graph of context nodes."]
#[derive(Debug, Eq, PartialEq)]
pub struct ContextGraph {
    nodes: Vec<ContextNode>,
    limits: ContextLimits,
}
impl ContextGraph {
    #[doc = " Validates canonical identity order, dependency existence, and acyclicity."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a stable error for an empty/oversized graph, duplicate or unordered IDs, missing"]
    #[doc = " dependencies, or any dependency cycle."]
    pub fn new(nodes: Vec<ContextNode>, limits: ContextLimits) -> Result<Self, ContextError> {
        if nodes.is_empty() {
            return Err(ContextError::plain(ContextErrorKind::EmptyCollection));
        }
        if nodes.len() > limits.max_nodes() {
            return Err(ContextError::with_numbers(
                ContextErrorKind::TooManyNodes,
                limits.max_nodes() as u64,
                nodes.len() as u64,
            ));
        }
        validation::validate_node_order(nodes.as_slice())?;
        validation::validate_dependencies(nodes.as_slice())?;
        if let Some(cycle_node) = cycle::cycle_member(nodes.as_slice()) {
            return Err(ContextError::node(ContextErrorKind::DependencyCycle, cycle_node));
        }
        Ok(Self { nodes, limits })
    }
    #[doc = " Borrows nodes in canonical identity order."]
    #[must_use]
    pub const fn nodes(&self) -> &[ContextNode] {
        self.nodes.as_slice()
    }
    #[doc = " Returns the construction limits."]
    #[must_use]
    pub const fn limits(&self) -> ContextLimits {
        self.limits
    }
    #[doc = " Finds one node by stable identity."]
    #[must_use]
    #[allow(
        clippy::option_if_let_else,
        reason = "explicit matching stays within Verus's supported executable subset"
    )]
    pub fn node(&self, id: ContextNodeId) -> Option<&ContextNode> {
        match validation::find_node_index(self.nodes.as_slice(), id) {
            Some(index) => Some(&self.nodes[index]),
            None => None,
        }
    }
    pub(crate) fn index_of(&self, id: ContextNodeId) -> Option<usize> {
        validation::find_node_index(self.nodes.as_slice(), id)
    }
}
impl Clone for ContextGraph {
    fn clone(&self) -> Self {
        Self { nodes: ContextNode::clone_sequence(self.nodes.as_slice()), limits: self.limits }
    }
}
