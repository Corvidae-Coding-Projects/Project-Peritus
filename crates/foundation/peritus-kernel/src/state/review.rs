//! Review-cycle lifecycle state.

use peritus_types::{AttemptId, ReviewCycleId, RunId};
use vstd::prelude::*;

verus! {

/// Lifecycle phase of one fresh-context review cycle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReviewPhase {
    /// Requested but not yet begun.
    Requested,
    /// A reviewer is actively evaluating the candidate.
    Active,
    /// Review observations were submitted.
    Submitted,
    /// A later candidate revision invalidated this cycle.
    Invalidated,
}

impl ReviewPhase {
    /// Returns whether this review can no longer advance.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Submitted | Self::Invalidated)
    }
}

/// Current state of one review cycle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReviewState {
    id: ReviewCycleId,
    run_id: RunId,
    attempt_id: AttemptId,
    phase: ReviewPhase,
}

impl ReviewState {
    pub(crate) const fn requested(
        id: ReviewCycleId,
        run_id: RunId,
        attempt_id: AttemptId,
    ) -> Self {
        Self { id, run_id, attempt_id, phase: ReviewPhase::Requested }
    }
    /// Returns the review-cycle identity.
    #[must_use]
    pub const fn id(self) -> ReviewCycleId { self.id }
    /// Returns the parent run.
    #[must_use]
    pub const fn run_id(self) -> RunId { self.run_id }
    /// Returns the reviewed attempt.
    #[must_use]
    pub const fn attempt_id(self) -> AttemptId { self.attempt_id }
    /// Returns the current phase.
    #[must_use]
    pub const fn phase(self) -> ReviewPhase { self.phase }
    pub(crate) const fn set_phase(&mut self, phase: ReviewPhase) { self.phase = phase; }
}

} // verus!
