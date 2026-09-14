//! Checked compaction proposals that preserve source ranges and complete lineage.

use crate::{
    CompactionPolicyId, ContextContent, ContextError, ContextErrorKind, ContextNode, ContextNodeId,
};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {
#[cfg(verus_only)]
mod model;
mod access;
mod validation;
mod replacement;
pub use validation::validate_compaction;
pub use replacement::{AppliedCompaction, replace_validated_compaction};
/// One nonempty half-open byte range bound to its source's complete digest.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceRange {
    source_id: ContextNodeId,
    source_digest: Sha256Digest,
    start: u64,
    end: u64,
}

impl SourceRange {
    /// Logical view of the exact source node identity.
    pub closed spec fn spec_source_id(self) -> ContextNodeId { self.source_id }

    /// Logical view of the exact complete source digest.
    pub closed spec fn spec_source_digest(self) -> Sha256Digest { self.source_digest }

    /// Logical view of the inclusive byte start.
    pub closed spec fn spec_start(self) -> u64 { self.start }

    /// Logical view of the exclusive byte end.
    pub closed spec fn spec_end(self) -> u64 { self.end }

    /// Whether the half-open range is nonempty.
    pub open spec fn spec_is_nonempty(self) -> bool { self.spec_start() < self.spec_end() }

    /// Creates a nonempty half-open range. Source length is checked during validation.
    ///
    /// # Errors
    ///
    /// Returns [`ContextErrorKind::InvalidSourceRange`] unless `start < end`.
    pub const fn new(
        source_id: ContextNodeId,
        source_digest: Sha256Digest,
        start: u64,
        end: u64,
    ) -> (result: Result<Self, ContextError>)
        ensures match result {
            Ok(range) => {
                &&& range.spec_source_id() == source_id
                &&& range.spec_source_digest() == source_digest
                &&& range.spec_start() == start
                &&& range.spec_end() == end
                &&& range.spec_is_nonempty()
            }
            Err(error) => error.spec_is_node(ContextErrorKind::InvalidSourceRange, source_id)
                && start >= end,
        },
    {
        if start >= end {
            Err(ContextError::node(ContextErrorKind::InvalidSourceRange, source_id))
        } else {
            Ok(Self { source_id, source_digest, start, end })
        }
    }

    /// Returns the source node identity.
    #[must_use]
    pub const fn source_id(self) -> (result: ContextNodeId)
        ensures result == self.spec_source_id(),
    {
        self.source_id
    }
    /// Returns the expected complete source digest.
    #[must_use]
    pub const fn source_digest(self) -> (result: Sha256Digest)
        ensures result == self.spec_source_digest(),
    {
        self.source_digest
    }
    /// Returns the inclusive start byte offset.
    #[must_use]
    pub const fn start(self) -> (result: u64)
        ensures result == self.spec_start(),
    {
        self.start
    }
    /// Returns the exclusive end byte offset.
    #[must_use]
    pub const fn end(self) -> (result: u64)
        ensures result == self.spec_end(),
    {
        self.end
    }
}

/// Immutable policy revision controlling whether all-trusted inputs retain trust.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactionPolicy {
    id: CompactionPolicyId,
    preserve_trust_for_all_trusted_sources: bool,
}

impl CompactionPolicy {
    /// Logical view of the exact policy revision.
    pub closed spec fn spec_id(self) -> CompactionPolicyId { self.id }

    /// Logical view of the all-trusted-source preservation rule.
    pub closed spec fn spec_preserves_trust(self) -> bool {
        self.preserve_trust_for_all_trusted_sources
    }

    /// Creates a policy revision from a caller-bound digest and trust rule.
    #[must_use]
    pub const fn new(
        id: CompactionPolicyId,
        preserve_trust_for_all_trusted_sources: bool,
    ) -> (result: Self)
        ensures
            result.spec_id() == id,
            result.spec_preserves_trust() == preserve_trust_for_all_trusted_sources,
    {
        Self { id, preserve_trust_for_all_trusted_sources }
    }

    /// Returns the exact policy ID.
    #[must_use]
    pub const fn id(self) -> (result: CompactionPolicyId)
        ensures result == self.spec_id(),
    {
        self.id
    }
    /// Whether every trusted input may yield trusted derived output.
    #[must_use]
    pub const fn preserves_trust(self) -> (result: bool)
        ensures result == self.spec_preserves_trust(),
    {
        self.preserve_trust_for_all_trusted_sources
    }
}

/// Bounded derived content and canonical source ranges proposed by a compactor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompactionProposal {
    node_id: ContextNodeId,
    policy_id: CompactionPolicyId,
    content: ContextContent,
    token_estimate: u64,
    recency_sequence: u64,
    priority: u16,
    source_ranges: Vec<SourceRange>,
}

impl CompactionProposal {
    /// Logical view of the derived node identity.
    pub closed spec fn spec_node_id(&self) -> ContextNodeId { self.node_id }

    /// Logical view of the named compaction policy.
    pub closed spec fn spec_policy_id(&self) -> CompactionPolicyId { self.policy_id }

    /// Logical view of the exact derived content.
    pub closed spec fn spec_content(&self) -> ContextContent { self.content }

    /// Logical view of the output token estimate.
    pub closed spec fn spec_token_estimate(&self) -> u64 { self.token_estimate }

    /// Logical view of the output recency sequence.
    pub closed spec fn spec_recency_sequence(&self) -> u64 { self.recency_sequence }

    /// Logical view of the output priority.
    pub closed spec fn spec_priority(&self) -> u16 { self.priority }

    /// Logical view of canonical source ranges.
    pub closed spec fn spec_source_ranges(&self) -> Seq<SourceRange> { self.source_ranges@ }

    /// Checks positive estimates and canonical nonoverlapping range order.
    ///
    /// # Errors
    ///
    /// Returns a typed error for zero token/recency values, empty ranges, duplicates,
    /// noncanonical ranges, or overlaps within a source.
    #[allow(clippy::too_many_arguments, reason = "proposal binds all derived-node and policy facts")]
    #[allow(
        clippy::suspicious_operation_groupings,
        reason = "overlap intentionally compares the prior end with the next start"
    )]
    pub fn new(
        node_id: ContextNodeId,
        policy_id: CompactionPolicyId,
        content: ContextContent,
        token_estimate: u64,
        recency_sequence: u64,
        priority: u16,
        source_ranges: Vec<SourceRange>,
    ) -> (result: Result<Self, ContextError>)
        ensures match result {
            Ok(proposal) => {
                &&& proposal.spec_node_id() == node_id
                &&& proposal.spec_policy_id() == policy_id
                &&& proposal.spec_content() == content
                &&& proposal.spec_token_estimate() == token_estimate
                &&& proposal.spec_recency_sequence() == recency_sequence
                &&& proposal.spec_priority() == priority
                &&& proposal.spec_source_ranges() == source_ranges@
                &&& token_estimate > 0
                &&& recency_sequence > 0
                &&& source_ranges@.len() > 0
            }
            Err(_) => true,
        },
    {
        if token_estimate == 0 {
            return Err(ContextError::node(ContextErrorKind::ZeroTokenEstimate, node_id));
        }
        if recency_sequence == 0 {
            return Err(ContextError::node(ContextErrorKind::ZeroRecency, node_id));
        }
        if source_ranges.is_empty() {
            return Err(ContextError::node(ContextErrorKind::EmptyCollection, node_id));
        }
        let mut index = 1;
        while index < source_ranges.len()
            invariant 1 <= index <= source_ranges.len(),
            decreases source_ranges.len() - index,
        {
            let previous = source_ranges[index - 1];
            let current = source_ranges[index];
            if previous == current {
                return Err(ContextError::nodes(
                    ContextErrorKind::DuplicateValue,
                    node_id,
                    current.source_id(),
                ));
            }
            if previous.source_id() > current.source_id()
                || (previous.source_id() == current.source_id()
                    && previous.start() > current.start())
            {
                return Err(ContextError::nodes(
                    ContextErrorKind::NonCanonicalOrder,
                    node_id,
                    current.source_id(),
                ));
            }
            if previous.source_id() == current.source_id() && previous.end() > current.start() {
                return Err(ContextError::nodes(
                    ContextErrorKind::OverlappingSourceRanges,
                    node_id,
                    current.source_id(),
                ));
            }
            index += 1;
        }
        Ok(Self {
            node_id,
            policy_id,
            content,
            token_estimate,
            recency_sequence,
            priority,
            source_ranges,
        })
    }

    /// Returns the new derived node identity.
    #[must_use]
    pub const fn node_id(&self) -> (result: ContextNodeId)
        ensures result == self.spec_node_id(),
    {
        self.node_id
    }
    /// Returns the named policy revision.
    #[must_use]
    pub const fn policy_id(&self) -> (result: CompactionPolicyId)
        ensures result == self.spec_policy_id(),
    {
        self.policy_id
    }
    /// Returns the bounded digest-verified output content.
    #[must_use]
    pub const fn content(&self) -> (result: &ContextContent)
        ensures *result == self.spec_content(),
    {
        &self.content
    }
    /// Returns the output token estimate.
    #[must_use]
    pub const fn token_estimate(&self) -> (result: u64)
        ensures result == self.spec_token_estimate(),
    {
        self.token_estimate
    }
    /// Returns canonical source ranges.
    #[must_use]
    pub const fn source_ranges(&self) -> (result: &[SourceRange])
        ensures result@ == self.spec_source_ranges(),
    {
        self.source_ranges.as_slice()
    }
}

/// Successfully validated derived node together with its exact source ranges and policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedCompaction {
    node: ContextNode,
    policy_id: CompactionPolicyId,
    source_ranges: Vec<SourceRange>,
    replaced_tokens: u64,
    sources: Vec<ValidatedSource>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ValidatedSource {
    node: ContextNode,
    required: bool,
}

impl ValidatedSource {
    closed spec fn spec_node(&self) -> ContextNode { self.node }

    closed spec fn spec_required(&self) -> bool { self.required }
}

impl ValidatedCompaction {
    /// Logical view of the checked derived node.
    pub closed spec fn spec_node(&self) -> ContextNode { self.node }

    /// Logical view of the policy revision used during validation.
    pub closed spec fn spec_policy_id(&self) -> CompactionPolicyId { self.policy_id }

    /// Logical view of exact source-range audit lineage.
    pub closed spec fn spec_source_ranges(&self) -> Seq<SourceRange> { self.source_ranges@ }

    /// Logical view of complete source-token replacement.
    pub closed spec fn spec_replaced_tokens(&self) -> u64 { self.replaced_tokens }

    /// Logical view of the exact canonical validated source identities.
    pub closed spec fn spec_source_ids(&self) -> Seq<ContextNodeId> {
        self.sources@.map_values(|source: ValidatedSource| source.spec_node().spec_id())
    }

    /// Logical view of every complete validated source node.
    pub closed spec fn spec_source_nodes(&self) -> Seq<ContextNode> {
        self.sources@.map_values(|source: ValidatedSource| source.spec_node())
    }

    /// Whether validation marked a source as required by mode or selection reason.
    pub closed spec fn spec_source_required(&self, index: int) -> bool {
        self.sources@[index].spec_required()
    }

    /// Whether validation established a strict token reduction.
    pub open spec fn spec_is_strict_reduction(&self) -> bool {
        self.spec_node().spec_metadata().spec_token_estimate() < self.spec_replaced_tokens()
    }

    /// Exact successful correspondence to the checked proposal and policy.
    pub open spec fn spec_matches_proposal(
        &self,
        proposal: &CompactionProposal,
        policy: CompactionPolicy,
    ) -> bool {
        &&& self.spec_node().spec_metadata().spec_id() == proposal.spec_node_id()
        &&& self.spec_node().spec_metadata().spec_provenance()
            == crate::Provenance::DerivedCompaction
        &&& self.spec_node().spec_metadata().spec_authority()
            == crate::AuthorityClass::NonAuthoritative
        &&& self.spec_node().spec_metadata().spec_content_kind()
            == crate::ContentKind::DerivedSummary
        &&& self.spec_node().spec_metadata().spec_token_estimate()
            == proposal.spec_token_estimate()
        &&& ContextContent::clone_equivalent(
            &proposal.spec_content(),
            &self.spec_node().spec_content(),
        )
        &&& self.spec_policy_id() == policy.spec_id()
        &&& self.spec_source_ranges() == proposal.spec_source_ranges()
        &&& self.spec_source_ids() == proposal.spec_source_ids()
        &&& self.spec_is_strict_reduction()
    }

}

} // verus!
