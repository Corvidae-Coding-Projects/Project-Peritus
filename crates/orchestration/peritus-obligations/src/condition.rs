//! Public-condition observations controlling conditional obligations.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::ConditionId;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Observed truth state of one public condition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ConditionState {
    /// The public condition holds and activates its obligations.
    Holds,
    /// The public condition demonstrably does not hold.
    DoesNotHold,
    /// The condition has not been resolved; qualification must fail closed.
    Unknown,
}

/// Evidence-backed observation of one public condition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ConditionObservation {
    condition_id: ConditionId,
    state: ConditionState,
    observation_digest: Sha256Digest,
}

impl ConditionObservation {
    /// Records one condition observation without assigning it candidate authority.
    #[must_use]
    pub const fn new(
        condition_id: ConditionId,
        state: ConditionState,
        observation_digest: Sha256Digest,
    ) -> Self {
        Self { condition_id, state, observation_digest }
    }

    /// Stable condition identity.
    #[must_use]
    pub const fn condition_id(self) -> ConditionId { self.condition_id }

    /// Observed truth state.
    #[must_use]
    pub const fn state(self) -> ConditionState { self.state }

    /// Digest of the public observation supporting the truth state.
    #[must_use]
    pub const fn observation_digest(self) -> Sha256Digest { self.observation_digest }
}

} // verus!
