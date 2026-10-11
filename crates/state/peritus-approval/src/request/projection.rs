//! Immutable challenge-time projections for bounded rendering.

use peritus_types::Generation;

impl super::ApprovalRequest {
    #[doc = " Returns the original policy-evaluation challenge epoch."]
    #[must_use]
    pub const fn challenge_epoch(&self) -> Generation {
        self.challenge_epoch
    }
    #[doc = " Returns the original policy-evaluation challenge tick."]
    #[must_use]
    pub const fn challenge_tick_millis(&self) -> u64 {
        self.challenge_tick_millis
    }
}
