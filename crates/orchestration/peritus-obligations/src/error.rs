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
    pub(crate) const fn plain(kind: ObligationErrorKind) -> Self {
        Self { kind, requirement_id: None, expected: None, actual: None }
    }

    pub(crate) const fn requirement(
        kind: ObligationErrorKind,
        requirement_id: RequirementId,
    ) -> Self {
        Self { kind, requirement_id: Some(requirement_id), expected: None, actual: None }
    }

    pub(crate) const fn numbers(
        kind: ObligationErrorKind,
        expected: u64,
        actual: u64,
    ) -> Self {
        Self { kind, requirement_id: None, expected: Some(expected), actual: Some(actual) }
    }

    /// Stable failure category.
    #[must_use]
    pub const fn kind(&self) -> ObligationErrorKind { self.kind }

    /// Relevant requirement identity, when one exists.
    #[must_use]
    pub const fn requirement_id(&self) -> Option<RequirementId> { self.requirement_id }

    /// Expected bound, when present.
    #[must_use]
    pub const fn expected(&self) -> Option<u64> { self.expected }

    /// Actual value, when present.
    #[must_use]
    pub const fn actual(&self) -> Option<u64> { self.actual }
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
