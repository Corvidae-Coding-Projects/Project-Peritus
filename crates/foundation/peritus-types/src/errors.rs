//! Stable errors returned by checked primitive constructors and arithmetic.

use vstd::prelude::*;

verus! {

/// Failure returned when constructing a nominal identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum IdentifierError {
    /// The all-zero byte pattern is reserved as an invalid identifier.
    Zero,
}

impl IdentifierError {
    /// Returns the stable diagnostic code for this error.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Zero => "PERITUS-TYPES-ID-001",
        }
    }
}

/// Failure returned when constructing or incrementing a one-based number.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OneBasedNumberError {
    /// Zero cannot represent a one-based number.
    Zero,
    /// Incrementing the maximum representable value would overflow.
    Overflow,
}

impl OneBasedNumberError {
    /// Returns the stable diagnostic code for this error.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Zero => "PERITUS-TYPES-NUMBER-001",
            Self::Overflow => "PERITUS-TYPES-NUMBER-002",
        }
    }
}

/// Failure returned when validating a capability name.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CapabilityNameError {
    /// The name has no bytes.
    Empty,
    /// The name exceeds [`crate::CapabilityName::MAX_LENGTH`] bytes.
    TooLong,
    /// A segment is missing before, after, or between separators.
    EmptySegment,
    /// A segment does not begin with an ASCII lowercase letter.
    InvalidSegmentStart,
    /// A non-separator byte is outside `[a-z0-9-]`.
    InvalidCharacter,
}

impl CapabilityNameError {
    /// Returns the stable diagnostic code for this error category.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Empty => "PERITUS-TYPES-CAPABILITY-001",
            Self::TooLong => "PERITUS-TYPES-CAPABILITY-002",
            Self::EmptySegment => "PERITUS-TYPES-CAPABILITY-003",
            Self::InvalidSegmentStart => "PERITUS-TYPES-CAPABILITY-004",
            Self::InvalidCharacter => "PERITUS-TYPES-CAPABILITY-005",
        }
    }
}

/// Failure returned by checked resource-quantity arithmetic.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ResourceQuantityError {
    /// Addition would exceed [`u64::MAX`].
    Overflow,
    /// Subtraction would produce a negative quantity.
    Underflow,
}

impl ResourceQuantityError {
    /// Returns the stable diagnostic code for this error.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Overflow => "PERITUS-TYPES-RESOURCE-001",
            Self::Underflow => "PERITUS-TYPES-RESOURCE-002",
        }
    }
}

} // verus!
