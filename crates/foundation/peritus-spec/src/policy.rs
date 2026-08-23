//! Bounded retry and review-cycle policy.

use crate::{LimitKind, SpecError};
use vstd::prelude::*;

verus! {

/// Checked completion limits; exhausting either limit is non-success.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CompletionPolicy {
    max_gate_attempts: u16,
    max_review_cycles: u16,
}

impl CompletionPolicy {
    /// Creates nonzero gate-attempt and review-cycle limits.
    ///
    /// # Errors
    ///
    /// Returns [`SpecError::ZeroLimit`] for the first zero value.
    pub const fn new(
        max_gate_attempts: u16,
        max_review_cycles: u16,
    ) -> Result<Self, SpecError> {
        if max_gate_attempts == 0 {
            return Err(SpecError::ZeroLimit(LimitKind::GateAttempts));
        }
        if max_review_cycles == 0 {
            return Err(SpecError::ZeroLimit(LimitKind::ReviewCycles));
        }
        Ok(Self { max_gate_attempts, max_review_cycles })
    }

    /// Returns the maximum permitted attempts for one gate.
    #[must_use]
    pub const fn max_gate_attempts(&self) -> u16 { self.max_gate_attempts }

    /// Returns the maximum writer/reviewer/fixer cycles.
    #[must_use]
    pub const fn max_review_cycles(&self) -> u16 { self.max_review_cycles }
}

} // verus!
