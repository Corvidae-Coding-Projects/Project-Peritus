//! Checked one-based completion ordinals carried by observations.

#[doc = " Error returned when an observation ordinal is not one-based."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ObservationOrdinalError;
#[doc = " One-based attempt number for a deterministic gate execution."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GateAttemptOrdinal {
    value: u16,
}
impl GateAttemptOrdinal {
    #[doc = " Creates a nonzero gate attempt ordinal."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`ObservationOrdinalError`] when `value` is zero."]
    pub const fn new(value: u16) -> Result<Self, ObservationOrdinalError> {
        if value == 0 { Err(ObservationOrdinalError) } else { Ok(Self { value }) }
    }
    #[doc = " Returns the one-based attempt number."]
    #[must_use]
    pub const fn get(self) -> u16 {
        self.value
    }
}
#[doc = " One-based ordinal of the review cycle that produced a review."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReviewCycleOrdinal {
    value: u16,
}
impl ReviewCycleOrdinal {
    #[doc = " Creates a nonzero review-cycle ordinal."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`ObservationOrdinalError`] when `value` is zero."]
    pub const fn new(value: u16) -> Result<Self, ObservationOrdinalError> {
        if value == 0 { Err(ObservationOrdinalError) } else { Ok(Self { value }) }
    }
    #[doc = " Returns the one-based review-cycle number."]
    #[must_use]
    pub const fn get(self) -> u16 {
        self.value
    }
}
