//! Immutable challenge-time projections for bounded rendering.

use peritus_types::Generation;
use vstd::prelude::*;

verus! {

impl super::ApprovalRequest {
    /// Returns the original policy-evaluation challenge epoch.
    #[must_use]
    pub const fn challenge_epoch(&self) -> (epoch: Generation)
        ensures epoch == self.spec_challenge_epoch(),
    { self.challenge_epoch }

    /// Returns the original policy-evaluation challenge tick.
    #[must_use]
    pub const fn challenge_tick_millis(&self) -> (tick: u64)
        ensures tick == self.spec_challenge_tick_millis(),
    { self.challenge_tick_millis }

    /// Returns the exact original challenge epoch used by specifications.
    pub closed spec fn spec_challenge_epoch(&self) -> Generation { self.challenge_epoch }

    /// Returns the exact original challenge tick used by specifications.
    pub closed spec fn spec_challenge_tick_millis(&self) -> u64 {
        self.challenge_tick_millis
    }
}

} // verus!
