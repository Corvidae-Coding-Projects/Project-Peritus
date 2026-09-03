//! Stable checked-construction and planning failures.

use crate::{KnowledgeSectionId, KnowledgeSourceId};
use vstd::prelude::*;

verus! {

/// Stable reason a knowledge snapshot or plan was rejected.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KnowledgeErrorKind {
    /// An identifier used the reserved all-zero representation.
    ZeroIdentifier,
    /// At least one configured bound was zero.
    InvalidLimit,
    /// A required collection was empty.
    EmptyCollection,
    /// A collection exceeded its configured bound.
    LimitExceeded,
    /// A collection was not in canonical increasing order.
    NonCanonicalOrder,
    /// A collection contained a duplicate identity.
    DuplicateValue,
    /// A section named itself as a dependency.
    SelfDependency,
    /// A dependency was absent or did not precede its consumer.
    InvalidDependency,
    /// A creation sequence used the reserved zero value.
    ZeroCreationSequence,
    /// The role is outside the writer-reviewer-fixer loop.
    UnsupportedRole,
    /// A section belongs to another run or workspace.
    CandidateLineageMismatch,
    /// A section belongs to another role-specific snapshot.
    RoleMismatch,
    /// A section was created after the snapshot checkpoint.
    FutureKnowledge,
    /// A required inventory, file-map, or requirement-ledger reference is absent or mistyped.
    InvalidRequiredSection,
    /// A clarification named a non-requirement or non-design section.
    InvalidClarificationTarget,
    /// Change-specific fields were present in an incompatible request.
    InvalidChangeRequest,
    /// A current snapshot still contains stale target-role knowledge.
    CurrentSnapshotStale,
}

/// Comparable failure with optional section, source, and numeric detail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KnowledgeError {
    kind: KnowledgeErrorKind,
    section_id: Option<KnowledgeSectionId>,
    source_id: Option<KnowledgeSourceId>,
    expected: Option<u64>,
    actual: Option<u64>,
}

impl KnowledgeError {
    pub(crate) const fn plain(kind: KnowledgeErrorKind) -> Self {
        Self { kind, section_id: None, source_id: None, expected: None, actual: None }
    }

    pub(crate) const fn section(kind: KnowledgeErrorKind, section_id: KnowledgeSectionId) -> Self {
        Self { kind, section_id: Some(section_id), source_id: None, expected: None, actual: None }
    }

    pub(crate) const fn source(kind: KnowledgeErrorKind, source_id: KnowledgeSourceId) -> Self {
        Self { kind, section_id: None, source_id: Some(source_id), expected: None, actual: None }
    }

    pub(crate) const fn numbers(kind: KnowledgeErrorKind, expected: u64, actual: u64) -> Self {
        Self {
            kind,
            section_id: None,
            source_id: None,
            expected: Some(expected),
            actual: Some(actual),
        }
    }

    /// Stable failure category.
    #[must_use]
    pub const fn kind(&self) -> KnowledgeErrorKind { self.kind }

    /// Relevant section identity, when present.
    #[must_use]
    pub const fn section_id(&self) -> Option<KnowledgeSectionId> { self.section_id }

    /// Relevant source identity, when present.
    #[must_use]
    pub const fn source_id(&self) -> Option<KnowledgeSourceId> { self.source_id }

    /// Expected bound or sequence, when present.
    #[must_use]
    pub const fn expected(&self) -> Option<u64> { self.expected }

    /// Observed bound or sequence, when present.
    #[must_use]
    pub const fn actual(&self) -> Option<u64> { self.actual }
}

} // verus!

#[cfg(not(verus_only))]
impl core::fmt::Display for KnowledgeError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "run knowledge rejected: {:?}", self.kind())
    }
}

#[cfg(not(verus_only))]
impl std::error::Error for KnowledgeError {}
