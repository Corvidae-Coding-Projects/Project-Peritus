//! Atomic graph replacement for already validated compaction evidence.

use super::{SourceRange, ValidatedCompaction};
use crate::{
    CompactionPolicyId, ContextError, ContextErrorKind, ContextGraph, ContextNodeId,
};
use vstd::prelude::*;

verus! {

/// Checked replacement result with audit lineage kept outside the live dependency graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppliedCompaction {
    graph: ContextGraph,
    policy_id: CompactionPolicyId,
    source_ids: Vec<ContextNodeId>,
    source_ranges: Vec<SourceRange>,
    replaced_tokens: u64,
    replacement_tokens: u64,
}

impl AppliedCompaction {
    /// Logical view of the exact replacement graph.
    pub closed spec fn spec_graph(&self) -> ContextGraph { self.graph }

    /// Logical view of the authorizing compaction policy.
    pub closed spec fn spec_policy_id(&self) -> CompactionPolicyId { self.policy_id }

    /// Logical view of the exact canonical source set removed from the live graph.
    pub closed spec fn spec_source_ids(&self) -> Seq<ContextNodeId> { self.source_ids@ }

    /// Logical view of complete source-range audit lineage.
    pub closed spec fn spec_source_ranges(&self) -> Seq<SourceRange> { self.source_ranges@ }

    /// Logical view of tokens removed from the live graph.
    pub closed spec fn spec_replaced_tokens(&self) -> u64 { self.replaced_tokens }

    /// Logical view of installed replacement tokens.
    pub closed spec fn spec_replacement_tokens(&self) -> u64 { self.replacement_tokens }

    /// Exact audit fields retained from validation, including strict reduction.
    pub open spec fn spec_matches_validation(&self, validated: &ValidatedCompaction) -> bool {
        &&& self.spec_policy_id() == validated.spec_policy_id()
        &&& self.spec_source_ranges() == validated.spec_source_ranges()
        &&& self.spec_replaced_tokens() == validated.spec_replaced_tokens()
        &&& self.spec_replacement_tokens() == validated.spec_node().spec_token_estimate()
        &&& self.spec_replacement_tokens() < self.spec_replaced_tokens()
    }

    /// Borrows the replacement graph.
    #[must_use]
    pub const fn graph(&self) -> (result: &ContextGraph)
        ensures *result == self.spec_graph(),
    {
        &self.graph
    }

    /// Returns the policy revision that authorized the derivation.
    #[must_use]
    pub const fn policy_id(&self) -> (result: CompactionPolicyId)
        ensures result == self.spec_policy_id(),
    {
        self.policy_id
    }

    /// Borrows the exact canonical set removed from the live graph.
    #[must_use]
    pub const fn source_ids(&self) -> (result: &[ContextNodeId])
        ensures result@ == self.spec_source_ids(),
    {
        self.source_ids.as_slice()
    }

    /// Borrows source-range audit lineage, which is not a live dependency closure.
    #[must_use]
    pub const fn source_ranges(&self) -> (result: &[SourceRange])
        ensures result@ == self.spec_source_ranges(),
    {
        self.source_ranges.as_slice()
    }

    /// Returns the complete token estimate removed from the live graph.
    #[must_use]
    pub const fn replaced_tokens(&self) -> (result: u64)
        ensures result == self.spec_replaced_tokens(),
    {
        self.replaced_tokens
    }

    /// Returns the installed derived node's token estimate.
    #[must_use]
    pub const fn replacement_tokens(&self) -> (result: u64)
        ensures result == self.spec_replacement_tokens(),
    {
        self.replacement_tokens
    }

    /// Consumes the audit result and returns the replacement graph.
    #[must_use]
    pub fn into_graph(self) -> ContextGraph { self.graph }
}

/// Atomically replaces the exact validated source set and rewrites all live dependency edges.
///
/// Source ranges remain in [`AppliedCompaction`] as audit lineage. The installed derived node
/// depends only on source dependencies outside the replaced set; every surviving dependent edge
/// targeting a source is deterministically rewritten to the derived node.
///
/// # Errors
///
/// Rejects graph drift, protected or required sources, non-reducing replacement, invalid rewritten
/// metadata, a missing dependency, or a dependency cycle. No partial graph is observable.
#[allow(
    clippy::too_many_lines,
    reason = "replacement and its source-bound proof remain one atomic transaction"
)]
pub fn replace_validated_compaction(
    graph: &ContextGraph,
    validated: ValidatedCompaction,
) -> (result: Result<AppliedCompaction, ContextError>)
    ensures match result {
        Ok(applied) => applied.spec_matches_validation(&validated),
        Err(_) => true,
    },
{
    let ghost expected_policy = validated.spec_policy_id();
    let ghost expected_ranges = validated.spec_source_ranges();
    let ghost expected_replaced_tokens = validated.spec_replaced_tokens();
    let ghost expected_replacement_tokens = validated.spec_node().spec_token_estimate();
    let output_id = validated.node.id();
    if graph.node(output_id).is_some() {
        return Err(ContextError::node(ContextErrorKind::CompactionNodeExists, output_id));
    }

    let mut source_ids = Vec::with_capacity(validated.sources.len());
    let mut replaced_tokens = 0u64;
    let mut source_index = 0;
    while source_index < validated.sources.len()
        invariant source_index <= validated.sources.len(),
        decreases validated.sources.len() - source_index,
    {
        let binding = &validated.sources[source_index];
        let source_id = binding.node.id();
        let Some(current) = graph.node(source_id) else {
            return Err(ContextError::nodes(
                ContextErrorKind::MissingCompactionSource,
                output_id,
                source_id,
            ));
        };
        if current != &binding.node {
            return Err(ContextError::nodes(
                ContextErrorKind::CompactionSourceChanged,
                output_id,
                source_id,
            ));
        }
        if current.content_kind().is_compaction_protected() {
            return Err(ContextError::nodes(
                ContextErrorKind::ProtectedCompactionSource,
                output_id,
                source_id,
            ));
        }
        if binding.required {
            return Err(ContextError::nodes(
                ContextErrorKind::RequiredCompactionSource,
                output_id,
                source_id,
            ));
        }
        replaced_tokens = replaced_tokens.checked_add(current.token_estimate()).ok_or_else(|| {
            ContextError::node(ContextErrorKind::ArithmeticOverflow, output_id)
        })?;
        source_ids.push(source_id);
        source_index += 1;
    }
    let replacement_tokens = validated.node.token_estimate();
    if replacement_tokens >= replaced_tokens || replaced_tokens != validated.replaced_tokens {
        return Err(ContextError::node_numbers(
            ContextErrorKind::CompactionNotSmaller,
            output_id,
            replaced_tokens.saturating_sub(1),
            replacement_tokens,
        ));
    }

    let live_dependencies = external_dependencies(&validated, source_ids.as_slice());
    let replacement = validated
        .node
        .replace_dependencies(live_dependencies, graph.limits())?;
    let graph_nodes = graph.nodes();
    let mut nodes = Vec::with_capacity(graph_nodes.len());
    let mut replacement_inserted = false;
    let mut node_index = 0;
    while node_index < graph_nodes.len()
        invariant node_index <= graph_nodes.len(),
        decreases graph_nodes.len() - node_index,
    {
        let node = &graph_nodes[node_index];
        if !replacement_inserted && output_id < node.id() {
            nodes.push(replacement.clone());
            replacement_inserted = true;
        }
        if !contains(source_ids.as_slice(), node.id()) {
            let dependencies = rewrite_dependencies(
                node.dependencies(),
                source_ids.as_slice(),
                output_id,
            );
            nodes.push(node.replace_dependencies(dependencies, graph.limits())?);
        }
        node_index += 1;
    }
    if !replacement_inserted {
        nodes.push(replacement);
    }
    let graph = ContextGraph::new(nodes, graph.limits())?;
    let applied = AppliedCompaction {
        graph,
        policy_id: validated.policy_id,
        source_ids,
        source_ranges: validated.source_ranges,
        replaced_tokens,
        replacement_tokens,
    };
    proof {
        reveal(AppliedCompaction::spec_policy_id);
        reveal(AppliedCompaction::spec_source_ranges);
        reveal(AppliedCompaction::spec_replaced_tokens);
        reveal(AppliedCompaction::spec_replacement_tokens);
        assert(applied.spec_policy_id() == expected_policy);
        assert(applied.spec_source_ranges() == expected_ranges);
        assert(applied.spec_replaced_tokens() == expected_replaced_tokens);
        assert(applied.spec_replacement_tokens() == expected_replacement_tokens);
        assert(applied.spec_replacement_tokens() < applied.spec_replaced_tokens());
        assert(applied.spec_matches_validation(&validated));
    }
    Ok(applied)
}

fn external_dependencies(
    validated: &ValidatedCompaction,
    source_ids: &[ContextNodeId],
) -> Vec<ContextNodeId> {
    let mut dependencies = Vec::new();
    let mut source_index = 0;
    while source_index < validated.sources.len()
        invariant source_index <= validated.sources.len(),
        decreases validated.sources.len() - source_index,
    {
        let source = &validated.sources[source_index];
        let source_dependencies = source.node.dependencies();
        let mut dependency_index = 0;
        while dependency_index < source_dependencies.len()
            invariant dependency_index <= source_dependencies.len(),
            decreases source_dependencies.len() - dependency_index,
        {
            let dependency = source_dependencies[dependency_index];
            if !contains(source_ids, dependency) {
                insert_canonical(&mut dependencies, dependency);
            }
            dependency_index += 1;
        }
        source_index += 1;
    }
    dependencies
}

fn rewrite_dependencies(
    dependencies: &[ContextNodeId],
    source_ids: &[ContextNodeId],
    output_id: ContextNodeId,
) -> Vec<ContextNodeId> {
    let mut rewritten = Vec::with_capacity(dependencies.len());
    let mut dependency_index = 0;
    while dependency_index < dependencies.len()
        invariant dependency_index <= dependencies.len(),
        decreases dependencies.len() - dependency_index,
    {
        let dependency = if contains(source_ids, dependencies[dependency_index]) {
            output_id
        } else {
            dependencies[dependency_index]
        };
        insert_canonical(&mut rewritten, dependency);
        dependency_index += 1;
    }
    rewritten
}

fn insert_canonical(values: &mut Vec<ContextNodeId>, value: ContextNodeId) {
    let mut position = 0;
    while position < values.len() && values[position] < value
        invariant position <= values.len(),
        decreases values.len() - position,
    {
        position += 1;
    }
    if position == values.len() || values[position] != value {
        values.insert(position, value);
    }
}

fn contains(values: &[ContextNodeId], target: ContextNodeId) -> bool {
    let mut index = 0;
    while index < values.len()
        invariant index <= values.len(),
        decreases values.len() - index,
    {
        if values[index] == target {
            return true;
        }
        index += 1;
    }
    false
}

} // verus!
