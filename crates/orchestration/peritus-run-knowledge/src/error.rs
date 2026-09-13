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
    /// Exact error category.
    pub closed spec fn spec_kind(&self) -> KnowledgeErrorKind { self.kind }
    /// Exact optional section identity.
    pub closed spec fn spec_section_id(&self) -> Option<KnowledgeSectionId> { self.section_id }
    /// Exact optional source identity.
    pub closed spec fn spec_source_id(&self) -> Option<KnowledgeSourceId> { self.source_id }
    /// Exact optional expected bound or sequence.
    pub closed spec fn spec_expected(&self) -> Option<u64> { self.expected }
    /// Exact optional observed bound or sequence.
    pub closed spec fn spec_actual(&self) -> Option<u64> { self.actual }

    /// A category-only error with every optional detail absent.
    pub open spec fn spec_plain(&self, kind: KnowledgeErrorKind) -> bool {
        self.spec_kind() == kind && self.spec_section_id().is_none()
            && self.spec_source_id().is_none() && self.spec_expected().is_none()
            && self.spec_actual().is_none()
    }

    /// An error retaining exactly the supplied section identity.
    pub open spec fn spec_section(&self, kind: KnowledgeErrorKind, id: KnowledgeSectionId) -> bool {
        self.spec_kind() == kind && self.spec_section_id() == Some(id)
            && self.spec_source_id().is_none() && self.spec_expected().is_none()
            && self.spec_actual().is_none()
    }

    /// An error retaining exactly the supplied source identity.
    pub open spec fn spec_source(&self, kind: KnowledgeErrorKind, id: KnowledgeSourceId) -> bool {
        self.spec_kind() == kind && self.spec_section_id().is_none()
            && self.spec_source_id() == Some(id) && self.spec_expected().is_none()
            && self.spec_actual().is_none()
    }

    /// An error retaining exactly the expected and observed numbers.
    pub open spec fn spec_numbers(&self, kind: KnowledgeErrorKind, expected: u64, actual: u64) -> bool {
        self.spec_kind() == kind && self.spec_section_id().is_none()
            && self.spec_source_id().is_none() && self.spec_expected() == Some(expected)
            && self.spec_actual() == Some(actual)
    }

    pub(crate) const fn plain(kind: KnowledgeErrorKind) -> (value: Self)
        ensures value.spec_plain(kind),
    {
        Self { kind, section_id: None, source_id: None, expected: None, actual: None }
    }

    pub(crate) const fn section(kind: KnowledgeErrorKind, section_id: KnowledgeSectionId) -> (value: Self)
        ensures value.spec_section(kind, section_id),
    {
        Self { kind, section_id: Some(section_id), source_id: None, expected: None, actual: None }
    }

    pub(crate) const fn source(kind: KnowledgeErrorKind, source_id: KnowledgeSourceId) -> (value: Self)
        ensures value.spec_source(kind, source_id),
    {
        Self { kind, section_id: None, source_id: Some(source_id), expected: None, actual: None }
    }

    pub(crate) const fn numbers(kind: KnowledgeErrorKind, expected: u64, actual: u64) -> (value: Self)
        ensures value.spec_numbers(kind, expected, actual),
    {
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
    pub const fn kind(&self) -> (value: KnowledgeErrorKind)
        ensures value == self.spec_kind(),
    { self.kind }

    /// Relevant section identity, when present.
    #[must_use]
    pub const fn section_id(&self) -> (value: Option<KnowledgeSectionId>)
        ensures value == self.spec_section_id(),
    { self.section_id }

    /// Relevant source identity, when present.
    #[must_use]
    pub const fn source_id(&self) -> (value: Option<KnowledgeSourceId>)
        ensures value == self.spec_source_id(),
    { self.source_id }

    /// Expected bound or sequence, when present.
    #[must_use]
    pub const fn expected(&self) -> (value: Option<u64>)
        ensures value == self.spec_expected(),
    { self.expected }

    /// Observed bound or sequence, when present.
    #[must_use]
    pub const fn actual(&self) -> (value: Option<u64>)
        ensures value == self.spec_actual(),
    { self.actual }
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
