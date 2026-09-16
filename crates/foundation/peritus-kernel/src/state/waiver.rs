//! Finding-waiver lifecycle state.

use peritus_types::{FindingId, ReviewCycleId, RunId};
use vstd::prelude::*;

verus! {

/// Lifecycle phase of one requested finding waiver.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WaiverPhase {
    /// Waiver authority was requested.
    Requested,
    /// Exact B2 evidence records an authorized grant.
    Granted,
    /// Waiver authority denied the request.
    Denied,
    /// A later candidate revision invalidated the waiver.
    Invalidated,
}

impl WaiverPhase {
    /// Returns whether the waiver cannot advance.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Granted | Self::Denied | Self::Invalidated)
    }
}

/// Current state of one finding waiver.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WaiverState {
    finding_id: FindingId,
    review_cycle_id: ReviewCycleId,
    run_id: RunId,
    phase: WaiverPhase,
}

impl WaiverState {
    /// Specification view of the target finding identity.
    pub closed spec fn spec_finding_id(&self) -> FindingId { self.finding_id }
    /// Specification view of the owning review-cycle identity.
    pub closed spec fn spec_review_cycle_id(&self) -> ReviewCycleId {
        self.review_cycle_id
    }
    /// Specification view of the parent run identity.
    pub closed spec fn spec_run_id(&self) -> RunId { self.run_id }
    /// Specification view of the current waiver phase.
    pub closed spec fn spec_phase(&self) -> WaiverPhase { self.phase }

    pub(crate) const fn requested(
        finding_id: FindingId,
        review_cycle_id: ReviewCycleId,
        run_id: RunId,
    ) -> (result: Self)
        ensures
            result.spec_finding_id() == finding_id,
            result.spec_review_cycle_id() == review_cycle_id,
            result.spec_run_id() == run_id,
            result.spec_phase() == WaiverPhase::Requested,
    {
        Self { finding_id, review_cycle_id, run_id, phase: WaiverPhase::Requested }
    }
    /// Returns the target finding.
    #[must_use]
    pub const fn finding_id(self) -> (finding_id: FindingId)
        ensures finding_id == self.spec_finding_id(),
    { self.finding_id }
    /// Returns the owning review cycle.
    #[must_use]
    pub const fn review_cycle_id(self) -> (review_cycle_id: ReviewCycleId)
        ensures review_cycle_id == self.spec_review_cycle_id(),
    { self.review_cycle_id }
    /// Returns the parent run.
    #[must_use]
    pub const fn run_id(self) -> (run_id: RunId)
        ensures run_id == self.spec_run_id(),
    { self.run_id }
    /// Returns the current phase.
    #[must_use]
    pub const fn phase(self) -> (phase: WaiverPhase)
        ensures phase == self.spec_phase(),
    { self.phase }
    pub(crate) const fn set_phase(&mut self, phase: WaiverPhase)
        ensures final(self).spec_finding_id() == old(self).spec_finding_id(),
            final(self).spec_review_cycle_id() == old(self).spec_review_cycle_id(),
            final(self).spec_run_id() == old(self).spec_run_id(),
            final(self).spec_phase() == phase,
    { self.phase = phase; }
}

} // verus!
