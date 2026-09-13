//! Stable checked-construction and qualification failures.

use peritus_spec::RequirementId;
use vstd::prelude::*;

verus! {

/// Stable reason an obligation value or qualification request was rejected.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ObligationErrorKind {
    /// At least one configured bound is invalid.
    InvalidLimit,
    /// Public task content is empty or oversized.
    InvalidSource,
    /// A public clause span is empty, out of bounds, or not canonical.
    InvalidClauseSpan,
    /// A retained name or path is empty or oversized.
    InvalidText,
    /// A collection exceeded its configured bound.
    LimitExceeded,
    /// A canonical identity collection is not strictly increasing.
    NonCanonicalOrder,
    /// A canonical identity collection contains a duplicate.
    DuplicateValue,
    /// A requirement class and its typed details disagree.
    RequirementShapeMismatch,
    /// A performance requirement or observation is structurally invalid.
    InvalidPerformance,
    /// A lifecycle requirement or observation is structurally invalid.
    InvalidLifecycle,
    /// A schema requirement or observation is structurally invalid.
    InvalidSchema,
    /// A browser requirement or observation is structurally invalid.
    InvalidBrowser,
    /// An external-effect observation is structurally invalid.
    InvalidExternalEffect,
    /// An alternative group does not contain at least two distinct branches.
    InvalidAlternative,
    /// Qualification received conflicting condition observations.
    InvalidCondition,
    /// Qualification received evidence for an unknown requirement.
    UnknownRequirement,
}

/// Comparable obligation error with optional requirement and numeric detail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObligationError {
    kind: ObligationErrorKind,
    requirement_id: Option<RequirementId>,
    expected: Option<u64>,
    actual: Option<u64>,
}

impl ObligationError {
    /// Exact error category.
    pub closed spec fn spec_kind(&self) -> ObligationErrorKind { self.kind }
    /// Exact optional requirement identity.
    pub closed spec fn spec_requirement_id(&self) -> Option<RequirementId> { self.requirement_id }
    /// Exact optional expected bound.
    pub closed spec fn spec_expected(&self) -> Option<u64> { self.expected }
    /// Exact optional actual value.
    pub closed spec fn spec_actual(&self) -> Option<u64> { self.actual }

    /// A category-only error with no hidden identity or numeric detail.
    pub open spec fn spec_plain(&self, kind: ObligationErrorKind) -> bool {
        self.spec_kind() == kind && self.spec_requirement_id().is_none()
            && self.spec_expected().is_none() && self.spec_actual().is_none()
    }

    /// An error retaining exactly one requirement identity.
    pub open spec fn spec_requirement(&self, kind: ObligationErrorKind, id: RequirementId) -> bool {
        self.spec_kind() == kind && self.spec_requirement_id() == Some(id)
            && self.spec_expected().is_none() && self.spec_actual().is_none()
    }

    /// An error retaining the exact expected and actual numeric details.
    pub open spec fn spec_numbers(&self, kind: ObligationErrorKind, expected: u64, actual: u64) -> bool {
        self.spec_kind() == kind && self.spec_requirement_id().is_none()
            && self.spec_expected() == Some(expected) && self.spec_actual() == Some(actual)
    }

    pub(crate) const fn plain(kind: ObligationErrorKind) -> (value: Self)
        ensures value.spec_plain(kind),
    {
        Self { kind, requirement_id: None, expected: None, actual: None }
    }

    pub(crate) const fn requirement(
        kind: ObligationErrorKind,
        requirement_id: RequirementId,
    ) -> (value: Self)
        ensures value.spec_requirement(kind, requirement_id),
    {
        Self { kind, requirement_id: Some(requirement_id), expected: None, actual: None }
    }

    pub(crate) const fn numbers(
        kind: ObligationErrorKind,
        expected: u64,
        actual: u64,
    ) -> (value: Self)
        ensures value.spec_numbers(kind, expected, actual),
    {
        Self { kind, requirement_id: None, expected: Some(expected), actual: Some(actual) }
    }

    /// Stable failure category.
    #[must_use]
    pub const fn kind(&self) -> (value: ObligationErrorKind)
        ensures value == self.spec_kind(),
    { self.kind }

    /// Relevant requirement identity, when one exists.
    #[must_use]
    pub const fn requirement_id(&self) -> (value: Option<RequirementId>)
        ensures value == self.spec_requirement_id(),
    { self.requirement_id }

    /// Expected bound, when present.
    #[must_use]
    pub const fn expected(&self) -> (value: Option<u64>)
        ensures value == self.spec_expected(),
    { self.expected }

    /// Actual value, when present.
    #[must_use]
    pub const fn actual(&self) -> (value: Option<u64>)
        ensures value == self.spec_actual(),
    { self.actual }
}

} // verus!

#[cfg(not(verus_only))]
impl core::fmt::Display for ObligationError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "obligation rejected: {:?}", self.kind())
    }
}

#[cfg(not(verus_only))]
impl std::error::Error for ObligationError {}
