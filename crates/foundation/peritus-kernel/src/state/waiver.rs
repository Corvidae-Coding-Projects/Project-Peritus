//! Finding-waiver lifecycle state.

use peritus_types::{FindingId, ReviewCycleId, RunId};

#[doc = " Lifecycle phase of one requested finding waiver."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WaiverPhase {
    #[doc = " Waiver authority was requested."]
    Requested,
    #[doc = " Exact B2 evidence records an authorized grant."]
    Granted,
    #[doc = " Waiver authority denied the request."]
    Denied,
    #[doc = " A later candidate revision invalidated the waiver."]
    Invalidated,
}
impl WaiverPhase {
    #[doc = " Returns whether the waiver cannot advance."]
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Granted | Self::Denied | Self::Invalidated)
    }
}
#[doc = " Current state of one finding waiver."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WaiverState {
    finding_id: FindingId,
    review_cycle_id: ReviewCycleId,
    run_id: RunId,
    phase: WaiverPhase,
}
impl WaiverState {
    pub(crate) const fn requested(
        finding_id: FindingId,
        review_cycle_id: ReviewCycleId,
        run_id: RunId,
    ) -> Self {
        Self { finding_id, review_cycle_id, run_id, phase: WaiverPhase::Requested }
    }
    #[doc = " Returns the target finding."]
    #[must_use]
    pub const fn finding_id(self) -> FindingId {
        self.finding_id
    }
    #[doc = " Returns the owning review cycle."]
    #[must_use]
    pub const fn review_cycle_id(self) -> ReviewCycleId {
        self.review_cycle_id
    }
    #[doc = " Returns the parent run."]
    #[must_use]
    pub const fn run_id(self) -> RunId {
        self.run_id
    }
    #[doc = " Returns the current phase."]
    #[must_use]
    pub const fn phase(self) -> WaiverPhase {
        self.phase
    }
    pub(crate) const fn set_phase(&mut self, phase: WaiverPhase) {
        self.phase = phase;
    }
}
