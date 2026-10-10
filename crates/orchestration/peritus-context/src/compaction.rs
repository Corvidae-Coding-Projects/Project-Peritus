//! Checked compaction proposals that preserve source ranges and complete lineage.

use crate::{
    CompactionPolicyId, ContextContent, ContextError, ContextErrorKind, ContextNode, ContextNodeId,
};
use peritus_types::Sha256Digest;

mod access;
mod replacement;
mod validation;
pub use replacement::{AppliedCompaction, replace_validated_compaction};
pub use validation::validate_compaction;
#[doc = " One nonempty half-open byte range bound to its source\'s complete digest."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceRange {
    source_id: ContextNodeId,
    source_digest: Sha256Digest,
    start: u64,
    end: u64,
}
impl SourceRange {
    #[doc = " Creates a nonempty half-open range. Source length is checked during validation."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`ContextErrorKind::InvalidSourceRange`] unless `start < end`."]
    pub const fn new(
        source_id: ContextNodeId,
        source_digest: Sha256Digest,
        start: u64,
        end: u64,
    ) -> Result<Self, ContextError> {
        if start >= end {
            Err(ContextError::node(ContextErrorKind::InvalidSourceRange, source_id))
        } else {
            Ok(Self { source_id, source_digest, start, end })
        }
    }
    #[doc = " Returns the source node identity."]
    #[must_use]
    pub const fn source_id(self) -> ContextNodeId {
        self.source_id
    }
    #[doc = " Returns the expected complete source digest."]
    #[must_use]
    pub const fn source_digest(self) -> Sha256Digest {
        self.source_digest
    }
    #[doc = " Returns the inclusive start byte offset."]
    #[must_use]
    pub const fn start(self) -> u64 {
        self.start
    }
    #[doc = " Returns the exclusive end byte offset."]
    #[must_use]
    pub const fn end(self) -> u64 {
        self.end
    }
}
#[doc = " Immutable policy revision controlling whether all-trusted inputs retain trust."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompactionPolicy {
    id: CompactionPolicyId,
    preserve_trust_for_all_trusted_sources: bool,
}
impl CompactionPolicy {
    #[doc = " Creates a policy revision from a caller-bound digest and trust rule."]
    #[must_use]
    pub const fn new(id: CompactionPolicyId, preserve_trust_for_all_trusted_sources: bool) -> Self {
        Self { id, preserve_trust_for_all_trusted_sources }
    }
    #[doc = " Returns the exact policy ID."]
    #[must_use]
    pub const fn id(self) -> CompactionPolicyId {
        self.id
    }
    #[doc = " Whether every trusted input may yield trusted derived output."]
    #[must_use]
    pub const fn preserves_trust(self) -> bool {
        self.preserve_trust_for_all_trusted_sources
    }
}
#[doc = " Bounded derived content and canonical source ranges proposed by a compactor."]
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
    #[doc = " Checks positive estimates and canonical nonoverlapping range order."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns a typed error for zero token/recency values, empty ranges, duplicates,"]
    #[doc = " noncanonical ranges, or overlaps within a source."]
    #[allow(
        clippy::too_many_arguments,
        reason = "proposal binds all derived-node and policy facts"
    )]
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
    ) -> Result<Self, ContextError> {
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
        while index < source_ranges.len() {
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
    #[doc = " Returns the new derived node identity."]
    #[must_use]
    pub const fn node_id(&self) -> ContextNodeId {
        self.node_id
    }
    #[doc = " Returns the named policy revision."]
    #[must_use]
    pub const fn policy_id(&self) -> CompactionPolicyId {
        self.policy_id
    }
    #[doc = " Returns the bounded digest-verified output content."]
    #[must_use]
    pub const fn content(&self) -> &ContextContent {
        &self.content
    }
    #[doc = " Returns the output token estimate."]
    #[must_use]
    pub const fn token_estimate(&self) -> u64 {
        self.token_estimate
    }
    #[doc = " Returns canonical source ranges."]
    #[must_use]
    pub const fn source_ranges(&self) -> &[SourceRange] {
        self.source_ranges.as_slice()
    }
}
#[doc = " Successfully validated derived node together with its exact source ranges and policy."]
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
impl ValidatedSource {}
impl ValidatedCompaction {}
