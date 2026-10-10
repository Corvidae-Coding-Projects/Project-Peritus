//! Resource dimensions and checked quantities.

// Explicit matches keep arithmetic proof branches direct and avoid verified closure machinery.
#![allow(clippy::option_if_let_else)]

use crate::ResourceQuantityError;

#[doc = " The unit and budget dimension measured by a [`ResourceQuantity`]."]
#[doc = ""]
#[doc = " The enum identifies units only; it grants no capability and makes no scheduling decision."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ResourceKind {
    #[doc = " Provider input and output tokens."]
    ModelTokens,
    #[doc = " Provider cost in integral millionths of the configured currency unit."]
    ProviderCostMicrounits,
    #[doc = " Elapsed wall-clock time in milliseconds."]
    WallTimeMilliseconds,
    #[doc = " Consumed CPU time in milliseconds."]
    CpuTimeMilliseconds,
    #[doc = " Resident or committed memory in bytes."]
    MemoryBytes,
    #[doc = " Persistent or temporary disk usage in bytes."]
    DiskBytes,
    #[doc = " Captured process or provider output in bytes."]
    OutputBytes,
    #[doc = " Owned process count."]
    ProcessCount,
    #[doc = " Concurrent execution slots."]
    ConcurrencySlots,
    #[doc = " Total execution-attempt count, including the initial attempt."]
    AttemptCount,
    #[doc = " Retry count."]
    RetryCount,
}
#[doc = " A nonnegative, integral resource quantity."]
#[doc = ""]
#[doc = " Zero is a valid value. Arithmetic never wraps or saturates: callers receive a typed error when"]
#[doc = " the mathematical result cannot be represented by `u64`."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResourceQuantity {
    value: u64,
}
impl ResourceQuantity {
    #[doc = " Returns the zero quantity."]
    #[must_use]
    pub const fn zero() -> Self {
        Self { value: 0 }
    }
    #[doc = " Creates a quantity. Every `u64`, including zero, is valid."]
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self { value }
    }
    #[doc = " Returns the primitive representation."]
    #[must_use]
    pub const fn get(self) -> u64 {
        self.value
    }
    #[doc = " Adds two quantities, rejecting an unrepresentable result."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`ResourceQuantityError::Overflow`] when the exact sum exceeds `u64`."]
    pub const fn checked_add(self, rhs: Self) -> Result<Self, ResourceQuantityError> {
        match self.value.checked_add(rhs.value) {
            Some(value) => Ok(Self { value }),
            None => Err(ResourceQuantityError::Overflow),
        }
    }
    #[doc = " Subtracts a quantity, rejecting a negative result."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`ResourceQuantityError::Underflow`] when `rhs` is greater than `self`."]
    pub const fn checked_sub(self, rhs: Self) -> Result<Self, ResourceQuantityError> {
        match self.value.checked_sub(rhs.value) {
            Some(value) => Ok(Self { value }),
            None => Err(ResourceQuantityError::Underflow),
        }
    }
}
