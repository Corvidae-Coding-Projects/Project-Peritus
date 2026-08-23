//! Checked one-based completion ordinals carried by observations.

use vstd::prelude::*;

verus! {

/// Error returned when an observation ordinal is not one-based.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ObservationOrdinalError;

/// One-based attempt number for a deterministic gate execution.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GateAttemptOrdinal {
    value: u16,
}

impl GateAttemptOrdinal {
    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool { self.value > 0 }

    /// Specification view of the one-based attempt number.
    pub closed spec fn spec_value(self) -> u16 { self.value }

    /// Creates a nonzero gate attempt ordinal.
    ///
    /// # Errors
    ///
    /// Returns [`ObservationOrdinalError`] when `value` is zero.
    pub const fn new(value: u16) -> (result: Result<Self, ObservationOrdinalError>)
        ensures
            result.is_ok() == (value > 0),
            match result {
                Ok(ordinal) => ordinal.spec_value() == value,
                Err(_) => true,
            },
    {
        if value == 0 {
            Err(ObservationOrdinalError)
        } else {
            Ok(Self { value })
        }
    }

    /// Returns the one-based attempt number.
    #[must_use]
    pub const fn get(self) -> (value: u16)
        ensures
            value == self.spec_value(),
            value > 0,
    {
        proof { use_type_invariant(&self); }
        self.value
    }
}

/// One-based ordinal of the review cycle that produced a review.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReviewCycleOrdinal {
    value: u16,
}

impl ReviewCycleOrdinal {
    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool { self.value > 0 }

    /// Specification view of the one-based review-cycle number.
    pub closed spec fn spec_value(self) -> u16 { self.value }

    /// Creates a nonzero review-cycle ordinal.
    ///
    /// # Errors
    ///
    /// Returns [`ObservationOrdinalError`] when `value` is zero.
    pub const fn new(value: u16) -> (result: Result<Self, ObservationOrdinalError>)
        ensures
            result.is_ok() == (value > 0),
            match result {
                Ok(ordinal) => ordinal.spec_value() == value,
                Err(_) => true,
            },
    {
        if value == 0 {
            Err(ObservationOrdinalError)
        } else {
            Ok(Self { value })
        }
    }

    /// Returns the one-based review-cycle number.
    #[must_use]
    pub const fn get(self) -> (value: u16)
        ensures
            value == self.spec_value(),
            value > 0,
    {
        proof { use_type_invariant(&self); }
        self.value
    }
}

} // verus!
