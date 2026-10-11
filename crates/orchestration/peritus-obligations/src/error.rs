//! Stable checked-construction and qualification failures.

use peritus_spec::RequirementId;

#[doc = " Stable reason an obligation value or qualification request was rejected."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ObligationErrorKind {
    #[doc = " At least one configured bound is invalid."]
    InvalidLimit,
    #[doc = " Public task content is empty or oversized."]
    InvalidSource,
    #[doc = " A public clause span is empty, out of bounds, or not canonical."]
    InvalidClauseSpan,
    #[doc = " A retained name or path is empty or oversized."]
    InvalidText,
    #[doc = " A collection exceeded its configured bound."]
    LimitExceeded,
    #[doc = " A canonical identity collection is not strictly increasing."]
    NonCanonicalOrder,
    #[doc = " A canonical identity collection contains a duplicate."]
    DuplicateValue,
    #[doc = " A requirement class and its typed details disagree."]
    RequirementShapeMismatch,
    #[doc = " A performance requirement or observation is structurally invalid."]
    InvalidPerformance,
    #[doc = " A lifecycle requirement or observation is structurally invalid."]
    InvalidLifecycle,
    #[doc = " A schema requirement or observation is structurally invalid."]
    InvalidSchema,
    #[doc = " A browser requirement or observation is structurally invalid."]
    InvalidBrowser,
    #[doc = " An external-effect observation is structurally invalid."]
    InvalidExternalEffect,
    #[doc = " An alternative group does not contain at least two distinct branches."]
    InvalidAlternative,
    #[doc = " Qualification received conflicting condition observations."]
    InvalidCondition,
    #[doc = " Qualification received evidence for an unknown requirement."]
    UnknownRequirement,
}
#[doc = " Comparable obligation error with optional requirement and numeric detail."]
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
    pub(crate) const fn numbers(kind: ObligationErrorKind, expected: u64, actual: u64) -> Self {
        Self { kind, requirement_id: None, expected: Some(expected), actual: Some(actual) }
    }
    #[doc = " Stable failure category."]
    #[must_use]
    pub const fn kind(&self) -> ObligationErrorKind {
        self.kind
    }
    #[doc = " Relevant requirement identity, when one exists."]
    #[must_use]
    pub const fn requirement_id(&self) -> Option<RequirementId> {
        self.requirement_id
    }
    #[doc = " Expected bound, when present."]
    #[must_use]
    pub const fn expected(&self) -> Option<u64> {
        self.expected
    }
    #[doc = " Actual value, when present."]
    #[must_use]
    pub const fn actual(&self) -> Option<u64> {
        self.actual
    }
}

impl core::fmt::Display for ObligationError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "obligation rejected: {:?}", self.kind())
    }
}

impl std::error::Error for ObligationError {}
