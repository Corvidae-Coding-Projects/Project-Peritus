//! Pure startup checkpoint sequencing and deterministic kill/restart failpoints.

mod checkpoint;
mod failpoint;

use core::fmt;

use peritus_types::Sha256Digest;

use crate::StartupPhase;

#[cfg(test)]
pub use checkpoint::StartupNextAction;
pub use checkpoint::{StartupCheckpoint, StartupCheckpoints};
#[cfg(test)]
pub use failpoint::{STARTUP_KILL_MATRIX, StartupFailpointBoundary};
pub use failpoint::{StartupFailpoint, StartupFailpoints};

/// Number of phases in the closed daemon startup plan.
pub const STARTUP_PHASE_COUNT: usize = 14;

/// Exact startup order shared by the effect owner and crash-recovery tests.
pub const STARTUP_PHASES: [StartupPhase; STARTUP_PHASE_COUNT] = [
    StartupPhase::Validate,
    StartupPhase::Lock,
    StartupPhase::Migrate,
    StartupPhase::Journal,
    StartupPhase::Artifacts,
    StartupPhase::Evidence,
    StartupPhase::Projections,
    StartupPhase::AuthorityEpoch,
    StartupPhase::DomainRecovery,
    StartupPhase::EffectRecovery,
    StartupPhase::AppRecovery,
    StartupPhase::Outbox,
    StartupPhase::Ipc,
    StartupPhase::Ready,
];

/// Exact injected stop retained by a halted startup execution.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StartupInjection {
    failpoint: StartupFailpoint,
    completed_checkpoints: u8,
}

impl StartupInjection {
    /// Returns the point that stopped execution.
    #[must_use]
    pub(crate) const fn failpoint(self) -> StartupFailpoint {
        self.failpoint
    }

    /// Returns the number of checkpoints preserved for restart.
    #[must_use]
    pub(crate) const fn completed_checkpoints(self) -> u8 {
        self.completed_checkpoints
    }

    /// Returns the exact phase a restart must execute next.
    #[must_use]
    pub(crate) fn restart_phase(self) -> Option<StartupPhase> {
        self.failpoint.restart_phase()
    }
}

/// Stable structural or injected failure from the pure startup plan.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StartupPlanError {
    /// A configured failpoint stopped execution.
    Injected(StartupInjection),
    /// A prior injection already halted this execution instance.
    Halted(StartupInjection),
    /// A caller tried to begin another phase while one was active.
    PhaseAlreadyActive { phase: StartupPhase },
    /// A caller tried to complete a phase before beginning it.
    NoActivePhase { observed: StartupPhase },
    /// A caller supplied a phase other than the exact active/next phase.
    UnexpectedPhase { expected: StartupPhase, observed: StartupPhase },
    /// Every bounded startup checkpoint is already present.
    AlreadyComplete,
}

impl StartupPlanError {
    /// Returns the stable machine-readable failure code.
    #[must_use]
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Injected(_) => "PERITUS-STARTUP-INJECTED-001",
            Self::Halted(_) => "PERITUS-STARTUP-HALTED-001",
            Self::PhaseAlreadyActive { .. } => "PERITUS-STARTUP-ACTIVE-001",
            Self::NoActivePhase { .. } => "PERITUS-STARTUP-INACTIVE-001",
            Self::UnexpectedPhase { .. } => "PERITUS-STARTUP-ORDER-001",
            Self::AlreadyComplete => "PERITUS-STARTUP-COMPLETE-001",
        }
    }
}

impl fmt::Display for StartupPlanError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {self:?}", self.code())
    }
}

impl std::error::Error for StartupPlanError {}

/// Pure monotonic executor for the closed startup plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StartupPlan {
    checkpoints: StartupCheckpoints,
    failpoints: StartupFailpoints,
    active: Option<StartupPhase>,
    halted: Option<StartupInjection>,
}

impl StartupPlan {
    /// Starts from validation with the supplied bounded failpoint set.
    #[must_use]
    pub(crate) const fn new(failpoints: StartupFailpoints) -> Self {
        Self::resume(StartupCheckpoints::empty(), failpoints)
    }

    /// Restarts from an exact checkpoint prefix with a fresh failpoint set.
    #[must_use]
    pub(crate) const fn resume(
        checkpoints: StartupCheckpoints,
        failpoints: StartupFailpoints,
    ) -> Self {
        Self { checkpoints, failpoints, active: None, halted: None }
    }

    /// Returns a copy of the exact bounded checkpoint prefix.
    #[must_use]
    pub(crate) const fn checkpoints(&self) -> StartupCheckpoints {
        self.checkpoints
    }

    /// Returns the phase currently executing, if any.
    #[must_use]
    pub(crate) const fn active_phase(&self) -> Option<StartupPhase> {
        self.active
    }

    /// Returns the injected stop retained by this execution instance.
    #[must_use]
    pub(crate) const fn halted(&self) -> Option<StartupInjection> {
        self.halted
    }

    /// Begins exactly the next canonical phase.
    ///
    /// # Errors
    ///
    /// Returns a stable structural error or the configured before-phase injection.
    pub(crate) fn begin_next(&mut self) -> Result<Option<StartupPhase>, StartupPlanError> {
        self.ensure_running()?;
        if let Some(phase) = self.active {
            return Err(StartupPlanError::PhaseAlreadyActive { phase });
        }
        let Some(phase) = self.checkpoints.next_phase() else {
            return Ok(None);
        };
        self.inject(StartupFailpoint::before(phase))?;
        self.active = Some(phase);
        Ok(Some(phase))
    }

    /// Retains exact phase digests and then applies the matching after-phase injection.
    ///
    /// # Errors
    ///
    /// Returns a stable structural error or the configured after-phase injection. An after-phase
    /// injection is reported only after its checkpoint has been retained for restart.
    pub(crate) fn complete_active(
        &mut self,
        observed: StartupPhase,
        input_digest: Sha256Digest,
        output_digest: Sha256Digest,
    ) -> Result<StartupCheckpoint, StartupPlanError> {
        self.ensure_running()?;
        let active = self.active.ok_or(StartupPlanError::NoActivePhase { observed })?;
        if observed != active {
            return Err(StartupPlanError::UnexpectedPhase { expected: active, observed });
        }
        let checkpoint = self.checkpoints.record(observed, input_digest, output_digest)?;
        self.active = None;
        self.inject(StartupFailpoint::after(observed))?;
        Ok(checkpoint)
    }

    fn ensure_running(self) -> Result<(), StartupPlanError> {
        self.halted.map_or(Ok(()), |injection| Err(StartupPlanError::Halted(injection)))
    }

    const fn inject(&mut self, failpoint: StartupFailpoint) -> Result<(), StartupPlanError> {
        if !self.failpoints.contains(failpoint) {
            return Ok(());
        }
        let injection =
            StartupInjection { failpoint, completed_checkpoints: self.checkpoints.len() };
        self.halted = Some(injection);
        Err(StartupPlanError::Injected(injection))
    }
}

fn phase_after(phase: StartupPhase) -> Option<StartupPhase> {
    STARTUP_PHASES.get(phase_index(phase) + 1).copied()
}

const fn phase_index(phase: StartupPhase) -> usize {
    match phase {
        StartupPhase::Validate => 0,
        StartupPhase::Lock => 1,
        StartupPhase::Migrate => 2,
        StartupPhase::Journal => 3,
        StartupPhase::Artifacts => 4,
        StartupPhase::Evidence => 5,
        StartupPhase::Projections => 6,
        StartupPhase::AuthorityEpoch => 7,
        StartupPhase::DomainRecovery => 8,
        StartupPhase::EffectRecovery => 9,
        StartupPhase::AppRecovery => 10,
        StartupPhase::Outbox => 11,
        StartupPhase::Ipc => 12,
        StartupPhase::Ready => 13,
    }
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
