//! Checked bounded integer scores used by validation and deterministic ranking.

use crate::{MemoryError, MemoryErrorKind, MemoryField};
use vstd::prelude::*;

verus! {

/// Inclusive upper bound for all basis-point values.
pub const MAX_BASIS_POINTS: u16 = 10_000;

/// A bounded integer value in `0..=10_000`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BasisPoints {
    pub(crate) value: u16,
}

impl BasisPoints {
    /// Zero basis points.
    pub(crate) const ZERO: Self = Self { value: 0 };
    /// Neutral midpoint used when no directional feedback exists.
    pub(crate) const NEUTRAL: Self = Self { value: 5_000 };
    /// Ten thousand basis points.
    pub(crate) const FULL: Self = Self { value: MAX_BASIS_POINTS };

    /// Creates a checked basis-point value.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryErrorKind::InvalidBound`] above 10,000.
    pub const fn new(value: u16) -> Result<Self, MemoryError> {
        if value > MAX_BASIS_POINTS {
            Err(MemoryError::field(MemoryErrorKind::InvalidBound, MemoryField::Score))
        } else {
            Ok(Self { value })
        }
    }

    /// Returns the primitive value.
    #[must_use]
    pub const fn get(self) -> u16 { self.value }
}

/// Evidence confidence represented as bounded integer basis points.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Confidence {
    value: BasisPoints,
}

impl Confidence {
    /// Creates checked confidence.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryErrorKind::InvalidBound`] above 10,000.
    pub const fn new(value: u16) -> Result<Self, MemoryError> {
        match BasisPoints::new(value) {
            Ok(value) => Ok(Self { value }),
            Err(error) => Err(error),
        }
    }

    /// Returns confidence in basis points.
    #[must_use]
    pub const fn basis_points(self) -> BasisPoints { self.value }
}

/// Nonzero bounded importance of one retrieval feature.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FeatureWeight {
    value: BasisPoints,
}

impl FeatureWeight {
    /// Creates a weight in `1..=10_000`.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryErrorKind::InvalidBound`] for zero or values above 10,000.
    pub const fn new(value: u16) -> Result<Self, MemoryError> {
        if value == 0 {
            return Err(MemoryError::field(MemoryErrorKind::InvalidBound, MemoryField::Score));
        }
        match BasisPoints::new(value) {
            Ok(value) => Ok(Self { value }),
            Err(error) => Err(error),
        }
    }

    /// Returns the feature weight in basis points.
    #[must_use]
    pub const fn basis_points(self) -> BasisPoints { self.value }
}

} // verus!
