//! Atomic graph replacement for already validated compaction evidence.

use super::{SourceRange, ValidatedCompaction};
#[cfg(verus_only)]
use super::ValidatedSource;
use crate::{
    CompactionPolicyId, ContextError, ContextErrorKind, ContextGraph, ContextNodeId,
};
use vstd::prelude::*;
pub mod model;
mod dependencies;
mod graph;
use graph::build_replacement_graph;
#[cfg(verus_only)]
use model::*;


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
    pub open spec fn spec_matches_validation(
        &self,
        before: &ContextGraph,
        validated: &ValidatedCompaction,
    ) -> bool {
        &&& self.spec_policy_id() == validated.spec_policy_id()
        &&& self.spec_source_ids() == validated.spec_source_ids()
        &&& self.spec_source_ranges() == validated.spec_source_ranges()
        &&& self.spec_replaced_tokens() == validated.spec_replaced_tokens()
        &&& self.spec_replacement_tokens() == validated.spec_node().spec_token_estimate()
        &&& self.spec_replacement_tokens() < self.spec_replaced_tokens()
        &&& exact_replacement_graph(before, validated, &self.spec_graph())
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
        Ok(applied) => applied.spec_matches_validation(graph, &validated),
        Err(_) => true,
    },
{
    let ghost expected_policy = validated.spec_policy_id();
    let ghost expected_source_ids = validated.spec_source_ids();
    let ghost expected_ranges = validated.spec_source_ranges();
    let ghost expected_replaced_tokens = validated.spec_replaced_tokens();
    let ghost expected_replacement_tokens = validated.spec_node().spec_token_estimate();
    let output_id = validated.node.id();
    let existing_output = graph.node(output_id);
    if existing_output.is_some() {
        return Err(ContextError::node(ContextErrorKind::CompactionNodeExists, output_id));
    }
    proof {
        assert(existing_output.is_none());
        assert(!graph.spec_contains_node(output_id));
    }

    let mut source_ids = Vec::with_capacity(validated.sources.len());
    let mut replaced_tokens = 0u64;
    let mut source_index = 0;
    proof {
        reveal(ValidatedCompaction::spec_source_ids);
        assert(expected_source_ids == validated.sources@.map_values(
            |source: ValidatedSource| source.spec_node().spec_id(),
        ));
        assert(expected_source_ids.len() == validated.sources@.len());
    }
    while source_index < validated.sources.len()
        invariant
            source_index <= validated.sources.len(),
            expected_source_ids == validated.sources@.map_values(
                |source: ValidatedSource| source.spec_node().spec_id(),
            ),
            !graph.spec_contains_node(output_id),
            source_ids@ == expected_source_ids.take(source_index as int),
            forall |prior: int| 0 <= prior < source_index ==>
                !expected_source_ids[prior].spec_matches(&output_id),
        decreases validated.sources.len() - source_index,
    {
        let binding = &validated.sources[source_index];
        let source_id = binding.node.id();
        proof {
            assert(*binding == validated.sources@[source_index as int]);
            assert(source_index < expected_source_ids.len());
            assert(expected_source_ids[source_index as int]
                == binding.spec_node().spec_id());
            assert(binding.spec_node().spec_id() == source_id);
        }
        let Some(current) = graph.node(source_id) else {
            return Err(ContextError::nodes(
                ContextErrorKind::MissingCompactionSource,
                output_id,
                source_id,
            ));
        };
        proof {
            assert(graph.spec_contains_node(source_id));
            if source_id.spec_matches(&output_id) {
                ContextNodeId::matches_implies_equal(&source_id, &output_id);
                assert(source_id == output_id);
                assert(graph.spec_contains_node(source_id)
                    == graph.spec_contains_node(output_id));
                assert(graph.spec_contains_node(output_id));
                assert(false);
            }
        }
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
        proof {
            assert(source_ids@ == expected_source_ids.take(source_index as int).push(source_id));
            assert(expected_source_ids.take(source_index as int).push(source_id)
                =~= expected_source_ids.take(source_index as int + 1));
        }
        source_index += 1;
    }
    proof {
        assert(source_index == validated.sources.len());
        assert(expected_source_ids.take(source_index as int) =~= expected_source_ids);
        assert(source_ids@ == expected_source_ids);
        assert(expected_source_ids == validated.spec_source_ids());
        assert(!contains_id(source_ids@, output_id)) by {
            reveal(contains_id);
            if contains_id(source_ids@, output_id) {
                let prior = choose |prior: int| #![trigger source_ids@[prior]]
                    0 <= prior < source_ids@.len()
                        && source_ids@[prior].spec_matches(&output_id);
                assert(false);
            }
        }
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

    let replacement_graph = build_replacement_graph(graph, &validated, source_ids.as_slice())?;
    let applied = AppliedCompaction {
        graph: replacement_graph,
        policy_id: validated.policy_id,
        source_ids,
        source_ranges: validated.source_ranges,
        replaced_tokens,
        replacement_tokens,
    };
    proof {
        reveal(AppliedCompaction::spec_policy_id);
        reveal(AppliedCompaction::spec_source_ids);
        reveal(AppliedCompaction::spec_source_ranges);
        reveal(AppliedCompaction::spec_replaced_tokens);
        reveal(AppliedCompaction::spec_replacement_tokens);
        assert(applied.spec_policy_id() == expected_policy);
        assert(applied.spec_source_ids() == expected_source_ids);
        assert(applied.spec_source_ranges() == expected_ranges);
        assert(applied.spec_replaced_tokens() == expected_replaced_tokens);
        assert(applied.spec_replacement_tokens() == expected_replacement_tokens);
        assert(applied.spec_replacement_tokens() < applied.spec_replaced_tokens());
        assert(applied.spec_matches_validation(graph, &validated));
    }
    Ok(applied)
}

} // verus!
