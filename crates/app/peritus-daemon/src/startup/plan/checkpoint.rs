//! Fixed-capacity startup checkpoint observations.

use peritus_types::Sha256Digest;

use super::{STARTUP_PHASE_COUNT, STARTUP_PHASES, StartupPlanError, phase_after};
use crate::StartupPhase;

/// Explicit action following a successfully retained startup checkpoint.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum StartupNextAction {
    /// Execute the named next phase.
    Execute(StartupPhase),
    /// Startup is fully checkpointed and no phase remains.
    Complete,
}

/// Exact observation retained after one startup phase completes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct StartupCheckpoint {
    ordinal: u8,
    phase: StartupPhase,
    input_digest: Sha256Digest,
    output_digest: Sha256Digest,
    next_action: StartupNextAction,
}

impl StartupCheckpoint {
    /// Returns the one-based checkpoint ordinal.
    #[must_use]
    pub(crate) const fn ordinal(self) -> u8 {
        self.ordinal
    }

    /// Returns the completed phase.
    #[must_use]
    pub(crate) const fn phase(self) -> StartupPhase {
        self.phase
    }

    /// Returns the exact digest supplied for phase inputs.
    #[must_use]
    pub(crate) const fn input_digest(self) -> Sha256Digest {
        self.input_digest
    }

    /// Returns the exact digest supplied for phase outputs.
    #[must_use]
    pub(crate) const fn output_digest(self) -> Sha256Digest {
        self.output_digest
    }

    /// Returns the explicit action following this checkpoint.
    #[must_use]
    pub(crate) const fn next_action(self) -> StartupNextAction {
        self.next_action
    }
}

/// Fixed-capacity, exact-prefix startup checkpoint log.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StartupCheckpoints {
    entries: [Option<StartupCheckpoint>; STARTUP_PHASE_COUNT],
    len: u8,
}

impl StartupCheckpoints {
    /// Creates an empty checkpoint prefix.
    #[must_use]
    pub(crate) const fn empty() -> Self {
        Self { entries: [None; STARTUP_PHASE_COUNT], len: 0 }
    }

    /// Returns the number of retained checkpoints.
    #[must_use]
    pub(crate) const fn len(self) -> u8 {
        self.len
    }

    /// Returns true when no checkpoint has been retained.
    #[must_use]
    pub(crate) const fn is_empty(self) -> bool {
        self.len == 0
    }

    /// Returns true when every canonical phase has checkpointed.
    #[must_use]
    pub(crate) fn is_complete(self) -> bool {
        usize::from(self.len) == STARTUP_PHASE_COUNT
    }

    /// Returns the exact next phase for this prefix.
    #[must_use]
    pub(crate) fn next_phase(self) -> Option<StartupPhase> {
        STARTUP_PHASES.get(usize::from(self.len)).copied()
    }

    /// Returns the latest retained checkpoint.
    #[must_use]
    pub(crate) fn last(self) -> Option<StartupCheckpoint> {
        usize::from(self.len).checked_sub(1).and_then(|index| self.entries[index])
    }

    /// Iterates over the initialized canonical prefix.
    pub(crate) fn iter(
        &self,
    ) -> std::iter::Flatten<std::slice::Iter<'_, Option<StartupCheckpoint>>> {
        self.entries[..usize::from(self.len)].iter().flatten()
    }

    pub(super) fn record(
        &mut self,
        phase: StartupPhase,
        input_digest: Sha256Digest,
        output_digest: Sha256Digest,
    ) -> Result<StartupCheckpoint, StartupPlanError> {
        let expected = self.next_phase().ok_or(StartupPlanError::AlreadyComplete)?;
        if phase != expected {
            return Err(StartupPlanError::UnexpectedPhase { expected, observed: phase });
        }
        let next_action =
            phase_after(phase).map_or(StartupNextAction::Complete, StartupNextAction::Execute);
        let checkpoint = StartupCheckpoint {
            ordinal: self.len + 1,
            phase,
            input_digest,
            output_digest,
            next_action,
        };
        self.entries[usize::from(self.len)] = Some(checkpoint);
        self.len += 1;
        Ok(checkpoint)
    }
}
