//! Session lifecycle state.

use peritus_types::SessionId;

#[doc = " Lifecycle phase of one durable user session."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SessionPhase {
    #[doc = " The session accepts new and resumed work."]
    Open,
    #[doc = " Intake is paused while state remains resumable."]
    Paused,
    #[doc = " The session is terminal and cannot reopen."]
    Closed,
}
impl SessionPhase {
    #[doc = " Returns whether this session is terminal."]
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Closed)
    }
}
#[doc = " Current state of one session."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SessionState {
    pub(crate) id: SessionId,
    pub(crate) phase: SessionPhase,
}
impl SessionState {
    pub(crate) const fn open(id: SessionId) -> Self {
        Self { id, phase: SessionPhase::Open }
    }
    #[doc = " Returns the session identity."]
    #[must_use]
    pub const fn id(self) -> SessionId {
        self.id
    }
    #[doc = " Returns the current phase."]
    #[must_use]
    pub const fn phase(self) -> SessionPhase {
        self.phase
    }
    pub(crate) const fn set_phase(&mut self, phase: SessionPhase) {
        self.phase = phase;
    }
}
