//! Explicit bounded positive and negative retrieval feedback.

use crate::{BasisPoints, MemoryError, MemoryErrorKind, MemoryField};
use vstd::prelude::*;

verus! {

/// Maximum observations retained in either feedback counter.
pub const MAX_FEEDBACK_COUNT: u16 = 10_000;

/// Bounded feedback summary. Negative observations remain visible and reduce rank.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Feedback {
    positive: u16,
    negative: u16,
}

impl Feedback {
    /// Creates a checked feedback summary.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryErrorKind::InvalidBound`] when either count exceeds 10,000.
    pub const fn new(positive: u16, negative: u16) -> Result<Self, MemoryError> {
        if positive > MAX_FEEDBACK_COUNT || negative > MAX_FEEDBACK_COUNT {
            Err(MemoryError::field(MemoryErrorKind::InvalidBound, MemoryField::Score))
        } else {
            Ok(Self { positive, negative })
        }
    }

    /// Returns a neutral summary with no observations.
    #[must_use]
    pub const fn none() -> Self { Self { positive: 0, negative: 0 } }

    /// Returns the positive observation count.
    #[must_use]
    pub const fn positive(self) -> u16 { self.positive }

    /// Returns the negative observation count.
    #[must_use]
    pub const fn negative(self) -> u16 { self.negative }

    /// Returns the negative share in basis points, or zero without observations.
    #[must_use]
    pub fn negative_ratio(self) -> BasisPoints {
        let total = u32::from(self.positive) + u32::from(self.negative);
        if total == 0 {
            return BasisPoints::ZERO;
        }
        let value = (u32::from(self.negative) * 10_000) / total;
        basis_or(value, BasisPoints::FULL)
    }

    /// Returns positive balance in basis points; absent feedback is neutral (5,000).
    #[must_use]
    pub fn rank_component(self) -> BasisPoints {
        let total = u32::from(self.positive) + u32::from(self.negative);
        if total == 0 {
            return BasisPoints::NEUTRAL;
        }
        let value = (u32::from(self.positive) * 10_000) / total;
        basis_or(value, BasisPoints::ZERO)
    }
}

fn basis_or(value: u32, fallback: BasisPoints) -> BasisPoints {
    let Ok(converted) = u16::try_from(value) else { return fallback };
    let Ok(points) = BasisPoints::new(converted) else { return fallback };
    points
}

} // verus!
