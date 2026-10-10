//! One-based revision, event-sequence, and generation numbers.

// Explicit matches keep the overflow proof branch-local and avoid an unnecessary verified closure.
#![allow(clippy::option_if_let_else)]

use crate::OneBasedNumberError;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct PositiveNumber {
    value: u64,
}
impl PositiveNumber {
    const fn new(value: u64) -> Result<Self, OneBasedNumberError> {
        if value == 0 { Err(OneBasedNumberError::Zero) } else { Ok(Self { value }) }
    }
    const fn get(self) -> u64 {
        self.value
    }
    const fn checked_next(self) -> Result<Self, OneBasedNumberError> {
        match self.value.checked_add(1) {
            Some(value) => Ok(Self { value }),
            None => Err(OneBasedNumberError::Overflow),
        }
    }
}
#[doc = " A one-based revision number for immutable domain revisions."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RevisionNumber(PositiveNumber);
impl RevisionNumber {
    #[doc = " Returns the first revision number."]
    #[must_use]
    pub const fn first() -> Self {
        Self(PositiveNumber { value: 1 })
    }
    #[doc = " Creates a revision number, rejecting zero."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`OneBasedNumberError::Zero`] when `value` is zero."]
    pub const fn new(value: u64) -> Result<Self, OneBasedNumberError> {
        match PositiveNumber::new(value) {
            Ok(number) => Ok(Self(number)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Returns the primitive representation."]
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
    #[doc = " Advances exactly once, returning overflow instead of wrapping."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`OneBasedNumberError::Overflow`] when the current value is [`u64::MAX`]."]
    pub const fn checked_next(self) -> Result<Self, OneBasedNumberError> {
        match self.0.checked_next() {
            Ok(number) => Ok(Self(number)),
            Err(error) => Err(error),
        }
    }
}
#[doc = " A one-based sequence number within one journal aggregate."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EventSequence(PositiveNumber);
impl EventSequence {
    #[doc = " Returns the first event sequence."]
    #[must_use]
    pub const fn first() -> Self {
        Self(PositiveNumber { value: 1 })
    }
    #[doc = " Creates an event sequence, rejecting zero."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`OneBasedNumberError::Zero`] when `value` is zero."]
    pub const fn new(value: u64) -> Result<Self, OneBasedNumberError> {
        match PositiveNumber::new(value) {
            Ok(number) => Ok(Self(number)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Returns the primitive representation."]
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
    #[doc = " Advances exactly once, returning overflow instead of wrapping."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`OneBasedNumberError::Overflow`] when the current value is [`u64::MAX`]."]
    pub const fn checked_next(self) -> Result<Self, OneBasedNumberError> {
        match self.0.checked_next() {
            Ok(number) => Ok(Self(number)),
            Err(error) => Err(error),
        }
    }
}
#[doc = " A one-based generation within a lineage."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Generation(PositiveNumber);
impl Generation {
    #[doc = " Returns the first generation."]
    #[must_use]
    pub const fn first() -> Self {
        Self(PositiveNumber { value: 1 })
    }
    #[doc = " Creates a generation, rejecting zero."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`OneBasedNumberError::Zero`] when `value` is zero."]
    pub const fn new(value: u64) -> Result<Self, OneBasedNumberError> {
        match PositiveNumber::new(value) {
            Ok(number) => Ok(Self(number)),
            Err(error) => Err(error),
        }
    }
    #[doc = " Returns the primitive representation."]
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
    #[doc = " Advances exactly once, returning overflow instead of wrapping."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`OneBasedNumberError::Overflow`] when the current value is [`u64::MAX`]."]
    pub const fn checked_next(self) -> Result<Self, OneBasedNumberError> {
        match self.0.checked_next() {
            Ok(number) => Ok(Self(number)),
            Err(error) => Err(error),
        }
    }
}
