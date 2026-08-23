//! Session lifecycle state.

use peritus_types::SessionId;
use vstd::prelude::*;

verus! {

/// Lifecycle phase of one durable user session.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SessionPhase {
    /// The session accepts new and resumed work.
    Open,
    /// Intake is paused while state remains resumable.
    Paused,
    /// The session is terminal and cannot reopen.
    Closed,
}

impl SessionPhase {
    /// Returns whether this session is terminal.
    #[must_use]
    pub const fn is_terminal(self) -> bool { matches!(self, Self::Closed) }
}

/// Current state of one session.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SessionState {
    pub(crate) id: SessionId,
    pub(crate) phase: SessionPhase,
}

impl SessionState {
    pub(crate) const fn open(id: SessionId) -> (result: Self)
        ensures result.id == id, result.phase == SessionPhase::Open,
    {
        Self { id, phase: SessionPhase::Open }
    }

    /// Returns the session identity.
    #[must_use]
    pub const fn id(self) -> SessionId { self.id }

    /// Returns the current phase.
    #[must_use]
    pub const fn phase(self) -> SessionPhase { self.phase }

    pub(crate) const fn set_phase(&mut self, phase: SessionPhase) { self.phase = phase; }
}

} // verus!
