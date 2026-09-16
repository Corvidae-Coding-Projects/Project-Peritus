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
    /// Specification view of the review-cycle identity.
    pub closed spec fn spec_id(&self) -> ReviewCycleId { self.id }
    /// Specification view of the parent run identity.
    pub closed spec fn spec_run_id(&self) -> RunId { self.run_id }
    /// Specification view of the reviewed attempt identity.
    pub closed spec fn spec_attempt_id(&self) -> AttemptId { self.attempt_id }
    /// Specification view of the current review phase.
    pub closed spec fn spec_phase(&self) -> ReviewPhase { self.phase }

    pub(crate) const fn requested(
        id: ReviewCycleId,
        run_id: RunId,
        attempt_id: AttemptId,
    ) -> (result: Self)
        ensures
            result.spec_id() == id,
            result.spec_run_id() == run_id,
            result.spec_attempt_id() == attempt_id,
            result.spec_phase() == ReviewPhase::Requested,
    {
        Self { id, run_id, attempt_id, phase: ReviewPhase::Requested }
    }
    /// Returns the review-cycle identity.
    #[must_use]
    pub const fn id(self) -> (id: ReviewCycleId)
        ensures id == self.spec_id(),
    { self.id }
    /// Returns the parent run.
    #[must_use]
    pub const fn run_id(self) -> (run_id: RunId)
        ensures run_id == self.spec_run_id(),
    { self.run_id }
    /// Returns the reviewed attempt.
    #[must_use]
    pub const fn attempt_id(self) -> (attempt_id: AttemptId)
        ensures attempt_id == self.spec_attempt_id(),
    { self.attempt_id }
    /// Returns the current phase.
    #[must_use]
    pub const fn phase(self) -> (phase: ReviewPhase)
        ensures phase == self.spec_phase(),
    { self.phase }
    pub(crate) const fn set_phase(&mut self, phase: ReviewPhase)
        ensures
            final(self).spec_id() == old(self).spec_id(),
            final(self).spec_run_id() == old(self).spec_run_id(),
            final(self).spec_attempt_id() == old(self).spec_attempt_id(),
            final(self).spec_phase() == phase,
    { self.phase = phase; }
}

} // verus!
