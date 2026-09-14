//! Stable, structured failures for context construction and planning.

use crate::ContextNodeId;
use vstd::prelude::*;

verus! {

/// Stable category for a context construction, planning, or compaction failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ContextErrorKind {
    /// An identifier used the reserved all-zero value.
    ZeroIdentifier,
    /// A configured bound was zero.
    InvalidLimit,
    /// Content was empty.
    EmptyContent,
    /// Content exceeded its byte bound.
    ContentTooLarge,
    /// A supplied digest did not match content or a source node.
    DigestMismatch,
    /// A token estimate was zero.
    ZeroTokenEstimate,
    /// Logical recency was zero.
    ZeroRecency,
    /// An explicitly ordered collection was empty.
    EmptyCollection,
    /// A collection was not in canonical increasing order.
    NonCanonicalOrder,
    /// A collection contained a duplicate.
    DuplicateValue,
    /// A node listed itself as a dependency.
    SelfDependency,
    /// A source exceeded its dependency bound.
    TooManyDependencies,
    /// A visibility set exceeded its role bound.
    TooManyVisibilityRoles,
    /// Provenance and authority were incompatible.
    IncompatibleAuthority,
    /// Provenance and trust were incompatible.
    IncompatibleTrust,
    /// Authority did not match a protected semantic content kind.
    IncompatibleContentKind,
    /// The graph exceeded its node bound.
    TooManyNodes,
    /// A graph dependency did not exist.
    MissingDependency,
    /// The graph contained a dependency cycle.
    DependencyCycle,
    /// Reserved output and protocol tokens exceeded the context window.
    InvalidTokenBudget,
    /// Checked integer arithmetic overflowed.
    ArithmeticOverflow,
    /// A required node was not visible to the selected role.
    HiddenRequiredNode,
    /// A required closure contained a hidden dependency.
    HiddenRequiredDependency,
    /// The required closure exceeded usable input tokens.
    RequiredTokenBudgetExceeded,
    /// The required closure exceeded the selected-node bound.
    RequiredNodeLimitExceeded,
    /// The required closure exceeded the selected-byte bound.
    RequiredByteLimitExceeded,
    /// A selection bound was invalid.
    InvalidSelectionPolicy,
    /// A selected plan referred to a graph node that was not present.
    PlanNodeMissing,
    /// Independent deterministic replay rejected a constructed selection result.
    SelectionCertificateMismatch,
    /// A compaction source range was empty or out of bounds.
    InvalidSourceRange,
    /// Source ranges overlapped.
    OverlappingSourceRanges,
    /// The output identity already existed in the graph.
    CompactionNodeExists,
    /// A compaction source was absent from the graph.
    MissingCompactionSource,
    /// A compaction source was not in the selected plan.
    CompactionSourceNotSelected,
    /// A source was hidden from the plan's role.
    HiddenCompactionSource,
    /// A protected class was proposed for summarization.
    ProtectedCompactionSource,
    /// Compaction lineage would create a cycle.
    CompactionSourceCycle,
    /// The output token estimate was not smaller than replaced selected material.
    CompactionNotSmaller,
    /// Compaction sources had incompatible context classes.
    IncompatibleCompactionClasses,
    /// The proposal named the wrong compaction-policy digest.
    CompactionPolicyMismatch,
    /// A source became required between selection and checked replacement.
    RequiredCompactionSource,
    /// A source's content or security/dependency metadata changed after validation.
    CompactionSourceChanged,
    /// A delta packet named a knowledge section absent from its current snapshot.
    KnowledgeSectionMissing,
    /// A delta packet section had no context-node binding.
    KnowledgeContextLinkMissing,
    /// A context node did not contain the exact knowledge-section bytes.
    KnowledgeContextDigestMismatch,
    /// Snapshot, delta packet, context visibility, or delivery authority crossed role boundaries.
    KnowledgeRoleMismatch,
}

/// Comparable context error with optional node and numeric detail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextError {
    kind: ContextErrorKind,
    node_id: Option<ContextNodeId>,
    related_id: Option<ContextNodeId>,
    expected: Option<u64>,
    actual: Option<u64>,
}

impl ContextError {
    /// Logical view of the stable failure category.
    pub closed spec fn spec_kind(&self) -> ContextErrorKind { self.kind }

    /// Logical view of the primary affected node.
    pub closed spec fn spec_node_id(&self) -> Option<ContextNodeId> { self.node_id }

    /// Logical view of the related dependency or source node.
    pub closed spec fn spec_related_id(&self) -> Option<ContextNodeId> { self.related_id }

    /// Logical view of the expected numeric value.
    pub closed spec fn spec_expected(&self) -> Option<u64> { self.expected }

    /// Logical view of the observed numeric value.
    pub closed spec fn spec_actual(&self) -> Option<u64> { self.actual }

    /// Exact plain error shape.
    pub open spec fn spec_is_plain(&self, kind: ContextErrorKind) -> bool {
        self.spec_kind() == kind
            && self.spec_node_id().is_none()
            && self.spec_related_id().is_none()
            && self.spec_expected().is_none()
            && self.spec_actual().is_none()
    }

    /// Exact single-node error shape.
    pub open spec fn spec_is_node(&self, kind: ContextErrorKind, node_id: ContextNodeId) -> bool {
        self.spec_kind() == kind
            && self.spec_node_id() == Some(node_id)
            && self.spec_related_id().is_none()
            && self.spec_expected().is_none()
            && self.spec_actual().is_none()
    }

    /// Exact two-node error shape.
    pub open spec fn spec_is_nodes(
        &self,
        kind: ContextErrorKind,
        node_id: ContextNodeId,
        related_id: ContextNodeId,
    ) -> bool {
        self.spec_kind() == kind
            && self.spec_node_id() == Some(node_id)
            && self.spec_related_id() == Some(related_id)
            && self.spec_expected().is_none()
            && self.spec_actual().is_none()
    }

    /// Exact numeric error shape.
    pub open spec fn spec_is_numbers(
        &self,
        kind: ContextErrorKind,
        expected: u64,
        actual: u64,
    ) -> bool {
        self.spec_kind() == kind
            && self.spec_node_id().is_none()
            && self.spec_related_id().is_none()
            && self.spec_expected() == Some(expected)
            && self.spec_actual() == Some(actual)
    }

    /// Exact node-and-numbers error shape.
    pub open spec fn spec_is_node_numbers(
        &self,
        kind: ContextErrorKind,
        node_id: ContextNodeId,
        expected: u64,
        actual: u64,
    ) -> bool {
        self.spec_kind() == kind
            && self.spec_node_id() == Some(node_id)
            && self.spec_related_id().is_none()
            && self.spec_expected() == Some(expected)
            && self.spec_actual() == Some(actual)
    }

    pub(crate) const fn plain(kind: ContextErrorKind) -> (error: Self)
        ensures error.spec_is_plain(kind),
    {
        Self { kind, node_id: None, related_id: None, expected: None, actual: None }
    }

    pub(crate) const fn node(kind: ContextErrorKind, node_id: ContextNodeId) -> (error: Self)
        ensures error.spec_is_node(kind, node_id),
    {
        Self {
            kind,
            node_id: Some(node_id),
            related_id: None,
            expected: None,
            actual: None,
        }
    }

    pub(crate) const fn nodes(
        kind: ContextErrorKind,
        node_id: ContextNodeId,
        related_id: ContextNodeId,
    ) -> (error: Self)
        ensures error.spec_is_nodes(kind, node_id, related_id),
    {
        Self {
            kind,
            node_id: Some(node_id),
            related_id: Some(related_id),
            expected: None,
            actual: None,
        }
    }

    pub(crate) const fn with_numbers(
        kind: ContextErrorKind,
        expected: u64,
        actual: u64,
    ) -> (error: Self)
        ensures error.spec_is_numbers(kind, expected, actual),
    {
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
    ) -> (error: Self)
        ensures error.spec_is_node_numbers(kind, node_id, expected, actual),
    {
        Self {
            kind,
            node_id: Some(node_id),
            related_id: None,
            expected: Some(expected),
            actual: Some(actual),
        }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> (kind: ContextErrorKind)
        ensures kind == self.spec_kind(),
    { self.kind }
    /// Returns the primary affected node, when applicable.
    #[must_use]
    pub const fn node_id(&self) -> (node_id: Option<ContextNodeId>)
        ensures node_id == self.spec_node_id(),
    { self.node_id }
    /// Returns the dependency or source node, when applicable.
    #[must_use]
    pub const fn related_id(&self) -> (related_id: Option<ContextNodeId>)
        ensures related_id == self.spec_related_id(),
    { self.related_id }
    /// Returns the expected bound or value, when applicable.
    #[must_use]
    pub const fn expected(&self) -> (expected: Option<u64>)
        ensures expected == self.spec_expected(),
    { self.expected }
    /// Returns the observed bound or value, when applicable.
    #[must_use]
    pub const fn actual(&self) -> (actual: Option<u64>)
        ensures actual == self.spec_actual(),
    { self.actual }
}

} // verus!
