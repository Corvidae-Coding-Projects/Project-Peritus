//! Resource dimensions and checked quantities.

// Explicit matches keep arithmetic proof branches direct and avoid verified closure machinery.
#![allow(clippy::option_if_let_else)]

use crate::ResourceQuantityError;
use vstd::prelude::*;

verus! {

/// The unit and budget dimension measured by a [`ResourceQuantity`].
///
/// The enum identifies units only; it grants no capability and makes no scheduling decision.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ResourceKind {
    /// Provider input and output tokens.
    ModelTokens,
    /// Provider cost in integral millionths of the configured currency unit.
    ProviderCostMicrounits,
    /// Elapsed wall-clock time in milliseconds.
    WallTimeMilliseconds,
    /// Consumed CPU time in milliseconds.
    CpuTimeMilliseconds,
    /// Resident or committed memory in bytes.
    MemoryBytes,
    /// Persistent or temporary disk usage in bytes.
    DiskBytes,
    /// Captured process or provider output in bytes.
    OutputBytes,
    /// Owned process count.
    ProcessCount,
    /// Concurrent execution slots.
    ConcurrencySlots,
    /// Total execution-attempt count, including the initial attempt.
    AttemptCount,
    /// Retry count.
    RetryCount,
}

/// A nonnegative, integral resource quantity.
///
/// Zero is a valid value. Arithmetic never wraps or saturates: callers receive a typed error when
/// the mathematical result cannot be represented by `u64`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceQuantity {
    value: u64,
}

impl ResourceQuantity {
    /// Returns the zero quantity.
    #[must_use]
    pub const fn zero() -> (quantity: Self) ensures quantity.spec_value() == 0 {
        Self { value: 0 }
    }

    /// Creates a quantity. Every `u64`, including zero, is valid.
    #[must_use]
    pub const fn new(value: u64) -> (quantity: Self)
        ensures
            quantity.spec_value() == value,
    {
        Self { value }
    }

    /// Returns the primitive representation.
    #[must_use]
    pub const fn get(self) -> (value: u64)
        ensures
            value == self.spec_value(),
    {
        self.value
    }

    /// Returns the mathematical quantity used by specifications.
    pub closed spec fn spec_value(&self) -> int {
        self.value as int
    }

    /// Adds two quantities, rejecting an unrepresentable result.
    ///
    /// # Errors
    ///
    /// Returns [`ResourceQuantityError::Overflow`] when the exact sum exceeds `u64`.
    pub const fn checked_add(self, rhs: Self) -> (result: Result<Self, ResourceQuantityError>)
        ensures
            match result {
                Ok(sum) => sum.spec_value() == self.spec_value() + rhs.spec_value(),
                Err(ResourceQuantityError::Overflow) =>
                    self.spec_value() + rhs.spec_value() > u64::MAX,
                Err(ResourceQuantityError::Underflow) => false,
            },
    {
        match self.value.checked_add(rhs.value) {
            Some(value) => Ok(Self { value }),
            None => Err(ResourceQuantityError::Overflow),
        }
    }

    /// Subtracts a quantity, rejecting a negative result.
    ///
    /// # Errors
    ///
    /// Returns [`ResourceQuantityError::Underflow`] when `rhs` is greater than `self`.
    pub const fn checked_sub(self, rhs: Self) -> (result: Result<Self, ResourceQuantityError>)
        ensures
            match result {
                Ok(difference) =>
                    difference.spec_value() == self.spec_value() - rhs.spec_value(),
                Err(ResourceQuantityError::Underflow) => self.spec_value() < rhs.spec_value(),
                Err(ResourceQuantityError::Overflow) => false,
            },
    {
        match self.value.checked_sub(rhs.value) {
            Some(value) => Ok(Self { value }),
            None => Err(ResourceQuantityError::Underflow),
        }
    }
}

} // verus!
