//! Turn lifecycle state.

use peritus_types::{AttemptId, TurnId};

#[doc = " Lifecycle phase of one model interaction turn."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TurnPhase {
    #[doc = " The turn may propose and complete actions."]
    Active,
    #[doc = " The turn completed successfully."]
    Completed,
    #[doc = " The turn failed."]
    Failed,
    #[doc = " The turn was cancelled before completion."]
    Cancelled,
}
impl TurnPhase {
    #[doc = " Returns whether the turn cannot advance."]
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}
#[doc = " Current state of one turn."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TurnState {
    id: TurnId,
    attempt_id: AttemptId,
    phase: TurnPhase,
}
impl TurnState {
    pub(crate) const fn active(id: TurnId, attempt_id: AttemptId) -> Self {
        Self { id, attempt_id, phase: TurnPhase::Active }
    }
    #[doc = " Returns the turn identity."]
    #[must_use]
    pub const fn id(self) -> TurnId {
        self.id
    }
    #[doc = " Returns the parent attempt."]
    #[must_use]
    pub const fn attempt_id(self) -> AttemptId {
        self.attempt_id
    }
    #[doc = " Returns the current phase."]
    #[must_use]
    pub const fn phase(self) -> TurnPhase {
        self.phase
    }
    pub(crate) const fn set_phase(&mut self, phase: TurnPhase) {
        self.phase = phase;
    }
}
