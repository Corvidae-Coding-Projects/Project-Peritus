//! Typed case observations and explicit assertion outcomes.

use crate::{AssertionFailure, ObservationId, ReportText};

/// A bounded-kind value suitable for deterministic conformance reports.
///
/// Raw provider or tool payloads are intentionally absent. Cases should report a redacted text
/// summary or a stable numeric/Boolean fact and retain sensitive bytes in their owning system.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ObservationValue {
    /// A Boolean fact.
    Boolean(bool),
    /// A signed integer fact.
    Signed(i64),
    /// An unsigned integer fact.
    Unsigned(u64),
    /// Caller-supplied validated and redacted text.
    Text(ReportText),
    /// Exact already-computed 32-byte digest observation.
    ///
    /// The variant does not hash content or claim authenticity.
    Digest([u8; 32]),
}

impl From<bool> for ObservationValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl From<i64> for ObservationValue {
    fn from(value: i64) -> Self {
        Self::Signed(value)
    }
}

impl From<u64> for ObservationValue {
    fn from(value: u64) -> Self {
        Self::Unsigned(value)
    }
}

impl From<ReportText> for ObservationValue {
    fn from(value: ReportText) -> Self {
        Self::Text(value)
    }
}

impl From<[u8; 32]> for ObservationValue {
    fn from(value: [u8; 32]) -> Self {
        Self::Digest(value)
    }
}

/// One typed, explicitly ordered observation produced by a case.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation {
    id: ObservationId,
    value: ObservationValue,
}

impl Observation {
    /// Creates an observation.
    #[must_use]
    pub const fn new(id: ObservationId, value: ObservationValue) -> Self {
        Self { id, value }
    }

    /// Returns the stable observation identifier.
    #[must_use]
    pub const fn id(&self) -> &ObservationId {
        &self.id
    }

    /// Returns the typed observed value.
    #[must_use]
    pub const fn value(&self) -> &ObservationValue {
        &self.value
    }
}

/// Explicit output of one conformance case.
///
/// Success is derived from the absence of a failure; callers cannot combine a success flag with a
/// contradictory failure. Observation order is retained exactly as supplied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaseResult {
    observations: Vec<Observation>,
    failure: Option<AssertionFailure>,
}

impl CaseResult {
    /// Creates a passing result with ordered observations.
    #[must_use]
    pub const fn passed(observations: Vec<Observation>) -> Self {
        Self { observations, failure: None }
    }

    /// Creates a failed result with ordered observations and one typed assertion failure.
    #[must_use]
    pub const fn failed(observations: Vec<Observation>, failure: AssertionFailure) -> Self {
        Self { observations, failure: Some(failure) }
    }

    /// Returns observations in the case-defined deterministic order.
    #[must_use]
    pub fn observations(&self) -> &[Observation] {
        &self.observations
    }

    /// Returns the failed assertion, or `None` when the case passed.
    #[must_use]
    pub const fn failure(&self) -> Option<&AssertionFailure> {
        self.failure.as_ref()
    }

    pub(crate) fn into_parts(self) -> (Vec<Observation>, Option<AssertionFailure>) {
        (self.observations, self.failure)
    }
}
