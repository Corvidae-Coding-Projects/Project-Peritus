//! Canonically ordered, bounded context dependency graph.

use crate::{ContextError, ContextErrorKind, ContextLimits, ContextNode, ContextNodeId};
use vstd::prelude::*;

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

verus! {

/// Immutable canonical directed acyclic graph of context nodes.
#[derive(Debug, Eq, PartialEq)]
pub struct ContextGraph {
    nodes: Vec<ContextNode>,
    limits: ContextLimits,
}

impl ContextGraph {
    /// Logical view of every node in canonical storage order.
    pub closed spec fn spec_nodes(&self) -> Seq<ContextNode> { self.nodes@ }

    /// Logical view of the bounds used at construction.
    pub closed spec fn spec_limits(&self) -> ContextLimits { self.limits }

    /// First exact node index for a supplied stable identity.
    pub open spec fn spec_node_index(&self, id: ContextNodeId) -> Option<nat> {
        model::first_node_index_from(self.spec_nodes(), id, 0)
    }

    /// Whether an exact node identity occurs in this graph.
    pub open spec fn spec_contains_node(&self, id: ContextNodeId) -> bool {
        self.spec_node_index(id).is_some()
    }

    /// Canonical identities and exact dependency resolution checked before cycle analysis.
    pub open spec fn spec_base_well_formed(&self) -> bool {
        0 < self.spec_nodes().len() <= self.spec_limits().spec_max_nodes()
            && model::nodes_canonical(self.spec_nodes())
            && model::dependencies_exist(self.spec_nodes())
    }

    /// Whether construction validated the exact traversal and produced a finite DAG certificate.
    pub open spec fn spec_is_well_formed(&self) -> bool {
        self.spec_base_well_formed()
            && cycle_model::acyclic(self.spec_nodes())
    }

    /// Validates canonical identity order, dependency existence, and acyclicity.
    ///
    /// # Errors
    ///
    /// Returns a stable error for an empty/oversized graph, duplicate or unordered IDs, missing
    /// dependencies, or any dependency cycle.
    pub fn new(
        nodes: Vec<ContextNode>,
        limits: ContextLimits,
    ) -> (result: Result<Self, ContextError>)
        ensures
            result.is_ok() <==> 0 < nodes@.len() <= limits.spec_max_nodes()
                && model::nodes_canonical(nodes@)
                && model::dependencies_exist(nodes@)
                && cycle_model::acyclic(nodes@),
            match result {
                Ok(graph) => graph.spec_nodes() == nodes@
                    && graph.spec_limits() == limits
                    && graph.spec_is_well_formed(),
                Err(error) => if nodes@.len() == 0 {
                    error.spec_is_plain(ContextErrorKind::EmptyCollection)
                } else if nodes@.len() > limits.spec_max_nodes() {
                    error.spec_is_numbers(
                        ContextErrorKind::TooManyNodes,
                        limits.spec_max_nodes() as u64,
                        nodes@.len() as u64,
                    )
                } else if !model::nodes_canonical(nodes@) {
                    match model::first_node_order_error(nodes@, 1) {
                        Some((kind, id)) => error.spec_is_node(kind, id),
                        None => false,
                    }
                } else if !model::dependencies_exist(nodes@) {
                    match model::first_missing_dependency(nodes@, 0) {
                        Some((owner, missing)) => error.spec_is_nodes(
                            ContextErrorKind::MissingDependency,
                            owner,
                            missing,
                        ),
                        None => false,
                    }
                } else if let Some(cycle_node) =
                    cycle_model::canonical_cycle_node(nodes@)
                {
                    error.spec_is_node(ContextErrorKind::DependencyCycle, cycle_node)
                } else {
                    false
                },
            },
    {
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

    /// Borrows nodes in canonical identity order.
    #[must_use]
    pub const fn nodes(&self) -> (nodes: &[ContextNode])
        ensures nodes@ == self.spec_nodes(),
    { self.nodes.as_slice() }

    /// Returns the construction limits.
    #[must_use]
    pub const fn limits(&self) -> (limits: ContextLimits)
        ensures limits == self.spec_limits(),
    { self.limits }

    /// Finds one node by stable identity.
    #[must_use]
    #[allow(
        clippy::option_if_let_else,
        reason = "explicit matching stays within Verus's supported executable subset"
    )]
    pub fn node(&self, id: ContextNodeId) -> (result: Option<&ContextNode>)
        ensures match result {
            Some(node) => node.spec_id().spec_matches(&id)
                && self.spec_contains_node(id),
            None => !self.spec_contains_node(id),
        },
    {
        proof {
            reveal(ContextGraph::spec_nodes);
            reveal(ContextGraph::spec_node_index);
        }
        match validation::find_node_index(self.nodes.as_slice(), id) {
            Some(index) => {
                assert(self.spec_nodes()[index as int].spec_id().spec_matches(&id));
                Some(&self.nodes[index])
            }
            None => None,
        }
    }

    pub(crate) fn index_of(&self, id: ContextNodeId) -> (result: Option<usize>)
        ensures match result {
            Some(index) => index < self.spec_nodes().len()
                && self.spec_node_index(id) == Some(index as nat),
            None => self.spec_node_index(id).is_none(),
        },
    {
        proof {
            reveal(ContextGraph::spec_nodes);
            reveal(ContextGraph::spec_node_index);
        }
        validation::find_node_index(self.nodes.as_slice(), id)
    }
}

impl Clone for ContextGraph {
    fn clone(&self) -> (result: Self)
        ensures
            ContextNode::sequence_clone_equivalent(self.spec_nodes(), result.spec_nodes()),
            result.spec_limits() == self.spec_limits(),
    {
        Self {
            nodes: ContextNode::clone_sequence(self.nodes.as_slice()),
            limits: self.limits,
        }
    }
}

} // verus!
