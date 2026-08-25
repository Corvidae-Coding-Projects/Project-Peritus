//! Canonically ordered, bounded context dependency graph.

use crate::{ContextError, ContextErrorKind, ContextLimits, ContextNode, ContextNodeId};
use vstd::prelude::*;

verus! {

/// Immutable canonical directed acyclic graph of context nodes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextGraph {
    nodes: Vec<ContextNode>,
    limits: ContextLimits,
}

impl ContextGraph {
    /// Validates canonical identity order, dependency existence, and acyclicity.
    ///
    /// # Errors
    ///
    /// Returns a stable error for an empty/oversized graph, duplicate or unordered IDs, missing
    /// dependencies, or any dependency cycle.
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
        let mut index = 1;
        while index < nodes.len()
            invariant 1 <= index <= nodes.len(),
            decreases nodes.len() - index,
        {
            if nodes[index - 1].id() == nodes[index].id() {
                return Err(ContextError::node(ContextErrorKind::DuplicateValue, nodes[index].id()));
            }
            if nodes[index - 1].id() > nodes[index].id() {
                return Err(ContextError::node(
                    ContextErrorKind::NonCanonicalOrder,
                    nodes[index].id(),
                ));
            }
            index += 1;
        }

        index = 0;
        while index < nodes.len()
            invariant index <= nodes.len(),
            decreases nodes.len() - index,
        {
            let dependencies = nodes[index].dependencies();
            let mut dependency_index = 0;
            while dependency_index < dependencies.len()
                invariant
                    dependency_index <= dependencies.len(),
                    index < nodes@.len(),
                decreases dependencies.len() - dependency_index,
            {
                if find_node_index(nodes.as_slice(), dependencies[dependency_index]).is_none() {
                    return Err(ContextError::nodes(
                        ContextErrorKind::MissingDependency,
                        nodes[index].id(),
                        dependencies[dependency_index],
                    ));
                }
                dependency_index += 1;
            }
            index += 1;
        }

        if let Some(cycle_node) = cycle_member(nodes.as_slice()) {
            return Err(ContextError::node(ContextErrorKind::DependencyCycle, cycle_node));
        }
        Ok(Self { nodes, limits })
    }

    /// Borrows nodes in canonical identity order.
    #[must_use]
    pub const fn nodes(&self) -> &[ContextNode] { self.nodes.as_slice() }

    /// Returns the construction limits.
    #[must_use]
    pub const fn limits(&self) -> ContextLimits { self.limits }

    /// Finds one node by stable identity.
    #[must_use]
    #[allow(
        clippy::option_if_let_else,
        reason = "explicit matching stays within Verus's supported executable subset"
    )]
    pub fn node(&self, id: ContextNodeId) -> Option<&ContextNode> {
        match find_node_index(self.nodes.as_slice(), id) {
            Some(index) => Some(&self.nodes[index]),
            None => None,
        }
    }

    pub(crate) fn index_of(&self, id: ContextNodeId) -> Option<usize> {
        find_node_index(self.nodes.as_slice(), id)
    }
}

fn find_node_index(nodes: &[ContextNode], id: ContextNodeId) -> (result: Option<usize>)
    ensures match result { Some(index) => index < nodes.len(), None => true },
{
    let mut index = 0;
    while index < nodes.len()
        invariant index <= nodes.len(),
        decreases nodes.len() - index,
    {
        if nodes[index].id() == id {
            return Some(index);
        }
        if nodes[index].id() > id {
            return None;
        }
        index += 1;
    }
    None
}

fn cycle_member(nodes: &[ContextNode]) -> Option<ContextNodeId> {
    let mut indegree = vec![0usize; nodes.len()];
    let mut node_index = 0;
    while node_index < nodes.len()
        invariant
            node_index <= nodes.len(),
            indegree.len() == nodes.len(),
        decreases nodes.len() - node_index,
    {
        let dependencies = nodes[node_index].dependencies();
        let mut dependency_index = 0;
        while dependency_index < dependencies.len()
            invariant
                dependency_index <= dependencies.len(),
                indegree.len() == nodes.len(),
                node_index < nodes@.len(),
            decreases dependencies.len() - dependency_index,
        {
            let Some(target) = find_node_index(nodes, dependencies[dependency_index]) else {
                return Some(nodes[node_index].id());
            };
            let Some(next) = indegree[target].checked_add(1) else {
                return Some(nodes[target].id());
            };
            indegree[target] = next;
            dependency_index += 1;
        }
        node_index += 1;
    }

    let mut removed = vec![false; nodes.len()];
    let mut removed_count = 0usize;
    while removed_count < nodes.len()
        invariant
            removed_count <= nodes.len(),
            indegree.len() == nodes.len(),
            removed.len() == nodes.len(),
        decreases nodes.len() - removed_count,
    {
        let mut found = None;
        node_index = 0;
        while node_index < nodes.len()
            invariant
                node_index <= nodes.len(),
                indegree.len() == nodes.len(),
                removed.len() == nodes.len(),
            decreases nodes.len() - node_index,
        {
            if !removed[node_index] && indegree[node_index] == 0 {
                found = Some(node_index);
                break;
            }
            node_index += 1;
        }
        let Some(index) = found else { break };
        if index >= nodes.len() {
            return None;
        }
        removed[index] = true;
        removed_count += 1;
        let dependencies = nodes[index].dependencies();
        let mut dependency_index = 0;
        while dependency_index < dependencies.len()
            invariant
                dependency_index <= dependencies.len(),
                indegree.len() == nodes.len(),
                index < nodes@.len(),
            decreases dependencies.len() - dependency_index,
        {
            let Some(target) = find_node_index(nodes, dependencies[dependency_index]) else {
                return Some(nodes[index].id());
            };
            let Some(next) = indegree[target].checked_sub(1) else {
                return Some(nodes[target].id());
            };
            indegree[target] = next;
            dependency_index += 1;
        }
    }
    if removed_count != nodes.len() {
        node_index = 0;
        while node_index < nodes.len()
            invariant
                node_index <= nodes.len(),
                removed.len() == nodes.len(),
            decreases nodes.len() - node_index,
        {
            if !removed[node_index] {
                return Some(nodes[node_index].id());
            }
            node_index += 1;
        }
    }
    None
}

} // verus!
