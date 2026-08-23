//! One-based revision, event-sequence, and generation numbers.

// Explicit matches keep the overflow proof branch-local and avoid an unnecessary verified closure.
#![allow(clippy::option_if_let_else)]

use crate::OneBasedNumberError;
use vstd::prelude::*;

verus! {

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct PositiveNumber {
    value: u64,
}

impl PositiveNumber {
    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool {
        self.value > 0
    }

    spec fn spec_value(&self) -> int {
        self.value as int
    }

    const fn new(value: u64) -> (result: Result<Self, OneBasedNumberError>)
        ensures
            result.is_ok() == (value > 0),
            match result {
                Ok(number) => number.spec_value() == value,
                Err(_) => true,
            },
    {
        if value == 0 {
            Err(OneBasedNumberError::Zero)
        } else {
            Ok(Self { value })
        }
    }

    const fn get(self) -> (value: u64)
        ensures
            value == self.spec_value(),
    {
        self.value
    }

    const fn checked_next(self) -> (result: Result<Self, OneBasedNumberError>)
        ensures
            match result {
                Ok(next) => {
                    next.spec_value() == self.spec_value() + 1
                        && self.spec_value() < u64::MAX as int
                }
                Err(OneBasedNumberError::Overflow) => self.spec_value() == u64::MAX,
                Err(OneBasedNumberError::Zero) => false,
            },
    {
        match self.value.checked_add(1) {
            Some(value) => Ok(Self { value }),
            None => Err(OneBasedNumberError::Overflow),
        }
    }

}

/// A one-based revision number for immutable domain revisions.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RevisionNumber(PositiveNumber);

impl RevisionNumber {
    /// Returns the first revision number.
    #[must_use]
    pub const fn first() -> (result: Self) ensures result.spec_value() == 1 {
        Self(PositiveNumber { value: 1 })
    }
    /// Creates a revision number, rejecting zero.
    ///
    /// # Errors
    ///
    /// Returns [`OneBasedNumberError::Zero`] when `value` is zero.
    pub const fn new(value: u64) -> (result: Result<Self, OneBasedNumberError>)
        ensures
            result.is_ok() == (value > 0),
            match result {
                Ok(number) => number.spec_value() == value,
                Err(_) => true,
            },
    { match PositiveNumber::new(value) { Ok(number) => Ok(Self(number)), Err(error) => Err(error) } }
    /// Returns the primitive representation.
    #[must_use]
    pub const fn get(self) -> (value: u64) ensures value == self.spec_value() { self.0.get() }
    /// Returns the mathematical value used by specifications.
    pub closed spec fn spec_value(&self) -> int { self.0.spec_value() }
    /// Returns whether the number satisfies its one-based invariant.
    pub closed spec fn is_valid(&self) -> bool { self.spec_value() > 0 }
    /// Advances exactly once, returning overflow instead of wrapping.
    ///
    /// # Errors
    ///
    /// Returns [`OneBasedNumberError::Overflow`] when the current value is [`u64::MAX`].
    pub const fn checked_next(self) -> (result: Result<Self, OneBasedNumberError>)
        ensures match result { Ok(next) => next.spec_value() == self.spec_value() + 1 && self.spec_value() < u64::MAX as int, Err(OneBasedNumberError::Overflow) => self.spec_value() == u64::MAX, Err(OneBasedNumberError::Zero) => false }
    { match self.0.checked_next() { Ok(number) => Ok(Self(number)), Err(error) => Err(error) } }
}

/// A one-based sequence number within one journal aggregate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EventSequence(PositiveNumber);

impl EventSequence {
    /// Returns the first event sequence.
    #[must_use]
    pub const fn first() -> (result: Self) ensures result.spec_value() == 1 {
        Self(PositiveNumber { value: 1 })
    }
    /// Creates an event sequence, rejecting zero.
    ///
    /// # Errors
    ///
    /// Returns [`OneBasedNumberError::Zero`] when `value` is zero.
    pub const fn new(value: u64) -> (result: Result<Self, OneBasedNumberError>)
        ensures
            result.is_ok() == (value > 0),
            match result {
                Ok(number) => number.spec_value() == value,
                Err(_) => true,
            },
    { match PositiveNumber::new(value) { Ok(number) => Ok(Self(number)), Err(error) => Err(error) } }
    /// Returns the primitive representation.
    #[must_use]
    pub const fn get(self) -> (value: u64) ensures value == self.spec_value() { self.0.get() }
    /// Returns the mathematical value used by specifications.
    pub closed spec fn spec_value(&self) -> int { self.0.spec_value() }
    /// Returns whether the number satisfies its one-based invariant.
    pub closed spec fn is_valid(&self) -> bool { self.spec_value() > 0 }
    /// Advances exactly once, returning overflow instead of wrapping.
    ///
    /// # Errors
    ///
    /// Returns [`OneBasedNumberError::Overflow`] when the current value is [`u64::MAX`].
    pub const fn checked_next(self) -> (result: Result<Self, OneBasedNumberError>)
        ensures match result { Ok(next) => next.spec_value() == self.spec_value() + 1 && self.spec_value() < u64::MAX as int, Err(OneBasedNumberError::Overflow) => self.spec_value() == u64::MAX, Err(OneBasedNumberError::Zero) => false }
    { match self.0.checked_next() { Ok(number) => Ok(Self(number)), Err(error) => Err(error) } }
}

/// A one-based generation within a lineage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Generation(PositiveNumber);

impl Generation {
    /// Returns the first generation.
    #[must_use]
    pub const fn first() -> (result: Self) ensures result.spec_value() == 1 {
        Self(PositiveNumber { value: 1 })
    }
    /// Creates a generation, rejecting zero.
    ///
    /// # Errors
    ///
    /// Returns [`OneBasedNumberError::Zero`] when `value` is zero.
    pub const fn new(value: u64) -> (result: Result<Self, OneBasedNumberError>)
        ensures
            result.is_ok() == (value > 0),
            match result {
                Ok(number) => number.spec_value() == value,
                Err(_) => true,
            },
    { match PositiveNumber::new(value) { Ok(number) => Ok(Self(number)), Err(error) => Err(error) } }
    /// Returns the primitive representation.
    #[must_use]
    pub const fn get(self) -> (value: u64) ensures value == self.spec_value() { self.0.get() }
    /// Returns the mathematical value used by specifications.
    pub closed spec fn spec_value(&self) -> int { self.0.spec_value() }
    /// Returns whether the number satisfies its one-based invariant.
    pub closed spec fn is_valid(&self) -> bool { self.spec_value() > 0 }
    /// Advances exactly once, returning overflow instead of wrapping.
    ///
    /// # Errors
    ///
    /// Returns [`OneBasedNumberError::Overflow`] when the current value is [`u64::MAX`].
    pub const fn checked_next(self) -> (result: Result<Self, OneBasedNumberError>)
        ensures match result { Ok(next) => next.spec_value() == self.spec_value() + 1 && self.spec_value() < u64::MAX as int, Err(OneBasedNumberError::Overflow) => self.spec_value() == u64::MAX, Err(OneBasedNumberError::Zero) => false }
    { match self.0.checked_next() { Ok(number) => Ok(Self(number)), Err(error) => Err(error) } }
}

} // verus!
