//! Atomic graph replacement for already validated compaction evidence.

use super::{SourceRange, ValidatedCompaction};

use crate::{CompactionPolicyId, ContextError, ContextErrorKind, ContextGraph, ContextNodeId};

mod dependencies;
mod graph;
use graph::build_replacement_graph;

#[doc = " Checked replacement result with audit lineage kept outside the live dependency graph."]
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
    #[doc = " Borrows the replacement graph."]
    #[must_use]
    pub const fn graph(&self) -> &ContextGraph {
        &self.graph
    }
    #[doc = " Returns the policy revision that authorized the derivation."]
    #[must_use]
    pub const fn policy_id(&self) -> CompactionPolicyId {
        self.policy_id
    }
    #[doc = " Borrows the exact canonical set removed from the live graph."]
    #[must_use]
    pub const fn source_ids(&self) -> &[ContextNodeId] {
        self.source_ids.as_slice()
    }
    #[doc = " Borrows source-range audit lineage, which is not a live dependency closure."]
    #[must_use]
    pub const fn source_ranges(&self) -> &[SourceRange] {
        self.source_ranges.as_slice()
    }
    #[doc = " Returns the complete token estimate removed from the live graph."]
    #[must_use]
    pub const fn replaced_tokens(&self) -> u64 {
        self.replaced_tokens
    }
    #[doc = " Returns the installed derived node\'s token estimate."]
    #[must_use]
    pub const fn replacement_tokens(&self) -> u64 {
        self.replacement_tokens
    }
    #[doc = " Consumes the audit result and returns the replacement graph."]
    #[must_use]
    pub fn into_graph(self) -> ContextGraph {
        self.graph
    }
}
#[doc = " Atomically replaces the exact validated source set and rewrites all live dependency edges."]
#[doc = ""]
#[doc = " Source ranges remain in [`AppliedCompaction`] as audit lineage. The installed derived node"]
#[doc = " depends only on source dependencies outside the replaced set; every surviving dependent edge"]
#[doc = " targeting a source is deterministically rewritten to the derived node."]
#[doc = ""]
#[doc = " # Errors"]
#[doc = ""]
#[doc = " Rejects graph drift, protected or required sources, non-reducing replacement, invalid rewritten"]
#[doc = " metadata, a missing dependency, or a dependency cycle. No partial graph is observable."]
#[allow(
    clippy::too_many_lines,
    reason = "replacement and its source-bound proof remain one atomic transaction"
)]
pub fn replace_validated_compaction(
    graph: &ContextGraph,
    validated: ValidatedCompaction,
) -> Result<AppliedCompaction, ContextError> {
    let output_id = validated.node.id();
    let existing_output = graph.node(output_id);
    if existing_output.is_some() {
        return Err(ContextError::node(ContextErrorKind::CompactionNodeExists, output_id));
    }
    let mut source_ids = Vec::with_capacity(validated.sources.len());
    let mut replaced_tokens = 0u64;
    let mut source_index = 0;
    while source_index < validated.sources.len() {
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
        replaced_tokens = replaced_tokens
            .checked_add(current.token_estimate())
            .ok_or_else(|| ContextError::node(ContextErrorKind::ArithmeticOverflow, output_id))?;
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
    let replacement_graph = build_replacement_graph(graph, &validated, source_ids.as_slice())?;
    let applied = AppliedCompaction {
        graph: replacement_graph,
        policy_id: validated.policy_id,
        source_ids,
        source_ranges: validated.source_ranges,
        replaced_tokens,
        replacement_tokens,
    };
    Ok(applied)
}
