//! Stable, structured failures for context construction and planning.

use crate::ContextNodeId;

#[doc = " Stable category for a context construction, planning, or compaction failure."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ContextErrorKind {
    #[doc = " An identifier used the reserved all-zero value."]
    ZeroIdentifier,
    #[doc = " A configured bound was zero."]
    InvalidLimit,
    #[doc = " Content was empty."]
    EmptyContent,
    #[doc = " Content exceeded its byte bound."]
    ContentTooLarge,
    #[doc = " A supplied digest did not match content or a source node."]
    DigestMismatch,
    #[doc = " A token estimate was zero."]
    ZeroTokenEstimate,
    #[doc = " Logical recency was zero."]
    ZeroRecency,
    #[doc = " An explicitly ordered collection was empty."]
    EmptyCollection,
    #[doc = " A collection was not in canonical increasing order."]
    NonCanonicalOrder,
    #[doc = " A collection contained a duplicate."]
    DuplicateValue,
    #[doc = " A node listed itself as a dependency."]
    SelfDependency,
    #[doc = " A source exceeded its dependency bound."]
    TooManyDependencies,
    #[doc = " A visibility set exceeded its role bound."]
    TooManyVisibilityRoles,
    #[doc = " Provenance and authority were incompatible."]
    IncompatibleAuthority,
    #[doc = " Provenance and trust were incompatible."]
    IncompatibleTrust,
    #[doc = " Authority did not match a protected semantic content kind."]
    IncompatibleContentKind,
    #[doc = " The graph exceeded its node bound."]
    TooManyNodes,
    #[doc = " A graph dependency did not exist."]
    MissingDependency,
    #[doc = " The graph contained a dependency cycle."]
    DependencyCycle,
    #[doc = " Reserved output and protocol tokens exceeded the context window."]
    InvalidTokenBudget,
    #[doc = " Checked integer arithmetic overflowed."]
    ArithmeticOverflow,
    #[doc = " A required node was not visible to the selected role."]
    HiddenRequiredNode,
    #[doc = " A required closure contained a hidden dependency."]
    HiddenRequiredDependency,
    #[doc = " The required closure exceeded usable input tokens."]
    RequiredTokenBudgetExceeded,
    #[doc = " The required closure exceeded the selected-node bound."]
    RequiredNodeLimitExceeded,
    #[doc = " The required closure exceeded the selected-byte bound."]
    RequiredByteLimitExceeded,
    #[doc = " A selection bound was invalid."]
    InvalidSelectionPolicy,
    #[doc = " A selected plan referred to a graph node that was not present."]
    PlanNodeMissing,
    #[doc = " Independent deterministic replay rejected a constructed selection result."]
    SelectionCertificateMismatch,
    #[doc = " A compaction source range was empty or out of bounds."]
    InvalidSourceRange,
    #[doc = " Source ranges overlapped."]
    OverlappingSourceRanges,
    #[doc = " The output identity already existed in the graph."]
    CompactionNodeExists,
    #[doc = " A compaction source was absent from the graph."]
    MissingCompactionSource,
    #[doc = " A compaction source was not in the selected plan."]
    CompactionSourceNotSelected,
    #[doc = " A source was hidden from the plan\'s role."]
    HiddenCompactionSource,
    #[doc = " A protected class was proposed for summarization."]
    ProtectedCompactionSource,
    #[doc = " Compaction lineage would create a cycle."]
    CompactionSourceCycle,
    #[doc = " The output token estimate was not smaller than replaced selected material."]
    CompactionNotSmaller,
    #[doc = " Compaction sources had incompatible context classes."]
    IncompatibleCompactionClasses,
    #[doc = " The proposal named the wrong compaction-policy digest."]
    CompactionPolicyMismatch,
    #[doc = " A source became required between selection and checked replacement."]
    RequiredCompactionSource,
    #[doc = " A source\'s content or security/dependency metadata changed after validation."]
    CompactionSourceChanged,
    #[doc = " A delta packet named a knowledge section absent from its current snapshot."]
    KnowledgeSectionMissing,
    #[doc = " A delta packet section had no context-node binding."]
    KnowledgeContextLinkMissing,
    #[doc = " A context node did not contain the exact knowledge-section bytes."]
    KnowledgeContextDigestMismatch,
    #[doc = " Snapshot, delta packet, context visibility, or delivery authority crossed role boundaries."]
    KnowledgeRoleMismatch,
}
#[doc = " Comparable context error with optional node and numeric detail."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextError {
    kind: ContextErrorKind,
    node_id: Option<ContextNodeId>,
    related_id: Option<ContextNodeId>,
    expected: Option<u64>,
    actual: Option<u64>,
}
impl ContextError {
    pub(crate) const fn plain(kind: ContextErrorKind) -> Self {
        Self { kind, node_id: None, related_id: None, expected: None, actual: None }
    }
    pub(crate) const fn node(kind: ContextErrorKind, node_id: ContextNodeId) -> Self {
        Self { kind, node_id: Some(node_id), related_id: None, expected: None, actual: None }
    }
    pub(crate) const fn nodes(
        kind: ContextErrorKind,
        node_id: ContextNodeId,
        related_id: ContextNodeId,
    ) -> Self {
        Self {
            kind,
            node_id: Some(node_id),
            related_id: Some(related_id),
            expected: None,
            actual: None,
        }
    }
    pub(crate) const fn with_numbers(kind: ContextErrorKind, expected: u64, actual: u64) -> Self {
        Self {
            kind,
            node_id: None,
            related_id: None,
            expected: Some(expected),
            actual: Some(actual),
        }
    }
    pub(crate) const fn node_numbers(
        kind: ContextErrorKind,
        node_id: ContextNodeId,
        expected: u64,
        actual: u64,
    ) -> Self {
        Self {
            kind,
            node_id: Some(node_id),
            related_id: None,
            expected: Some(expected),
            actual: Some(actual),
        }
    }
    #[doc = " Returns the stable failure category."]
    #[must_use]
    pub const fn kind(&self) -> ContextErrorKind {
        self.kind
    }
    #[doc = " Returns the primary affected node, when applicable."]
    #[must_use]
    pub const fn node_id(&self) -> Option<ContextNodeId> {
        self.node_id
    }
    #[doc = " Returns the dependency or source node, when applicable."]
    #[must_use]
    pub const fn related_id(&self) -> Option<ContextNodeId> {
        self.related_id
    }
    #[doc = " Returns the expected bound or value, when applicable."]
    #[must_use]
    pub const fn expected(&self) -> Option<u64> {
        self.expected
    }
    #[doc = " Returns the observed bound or value, when applicable."]
    #[must_use]
    pub const fn actual(&self) -> Option<u64> {
        self.actual
    }
}
