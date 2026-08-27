//! Fixed-size failpoint vocabulary for startup kill/restart testing.

use super::{STARTUP_PHASE_COUNT, phase_after, phase_index};
use crate::StartupPhase;

/// Side of a startup phase at which a deterministic process kill is injected.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum StartupFailpointBoundary {
    /// Stop immediately before the phase begins and before it has a checkpoint.
    Before,
    /// Stop immediately after the phase checkpoint has been retained.
    After,
}

/// One stable injection point in the startup kill/restart matrix.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct StartupFailpoint {
    phase: StartupPhase,
    boundary: StartupFailpointBoundary,
}

impl StartupFailpoint {
    /// Creates a failpoint before the selected phase.
    #[must_use]
    pub(crate) const fn before(phase: StartupPhase) -> Self {
        Self { phase, boundary: StartupFailpointBoundary::Before }
    }

    /// Creates a failpoint after the selected phase has checkpointed.
    #[must_use]
    pub(crate) const fn after(phase: StartupPhase) -> Self {
        Self { phase, boundary: StartupFailpointBoundary::After }
    }

    /// Returns the affected startup phase.
    #[must_use]
    pub(crate) const fn phase(self) -> StartupPhase {
        self.phase
    }

    /// Returns whether the process stops before or after the phase checkpoint.
    #[must_use]
    pub(crate) const fn boundary(self) -> StartupFailpointBoundary {
        self.boundary
    }

    /// Returns the phase a restart must execute next, or `None` after readiness checkpointing.
    #[must_use]
    pub(crate) fn restart_phase(self) -> Option<StartupPhase> {
        match self.boundary {
            StartupFailpointBoundary::Before => Some(self.phase),
            StartupFailpointBoundary::After => phase_after(self.phase),
        }
    }
}

/// Every meaningful before/after injection point in canonical execution order.
pub(crate) const STARTUP_KILL_MATRIX: [StartupFailpoint; STARTUP_PHASE_COUNT * 2] = [
    StartupFailpoint::before(StartupPhase::Validate),
    StartupFailpoint::after(StartupPhase::Validate),
    StartupFailpoint::before(StartupPhase::Lock),
    StartupFailpoint::after(StartupPhase::Lock),
    StartupFailpoint::before(StartupPhase::Migrate),
    StartupFailpoint::after(StartupPhase::Migrate),
    StartupFailpoint::before(StartupPhase::Journal),
    StartupFailpoint::after(StartupPhase::Journal),
    StartupFailpoint::before(StartupPhase::Artifacts),
    StartupFailpoint::after(StartupPhase::Artifacts),
    StartupFailpoint::before(StartupPhase::Evidence),
    StartupFailpoint::after(StartupPhase::Evidence),
    StartupFailpoint::before(StartupPhase::Projections),
    StartupFailpoint::after(StartupPhase::Projections),
    StartupFailpoint::before(StartupPhase::AuthorityEpoch),
    StartupFailpoint::after(StartupPhase::AuthorityEpoch),
    StartupFailpoint::before(StartupPhase::DomainRecovery),
    StartupFailpoint::after(StartupPhase::DomainRecovery),
    StartupFailpoint::before(StartupPhase::EffectRecovery),
    StartupFailpoint::after(StartupPhase::EffectRecovery),
    StartupFailpoint::before(StartupPhase::AppRecovery),
    StartupFailpoint::after(StartupPhase::AppRecovery),
    StartupFailpoint::before(StartupPhase::Outbox),
    StartupFailpoint::after(StartupPhase::Outbox),
    StartupFailpoint::before(StartupPhase::Ipc),
    StartupFailpoint::after(StartupPhase::Ipc),
    StartupFailpoint::before(StartupPhase::Ready),
    StartupFailpoint::after(StartupPhase::Ready),
];

/// Fixed-size set of enabled startup failpoints.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct StartupFailpoints {
    before: u16,
    after: u16,
}

impl StartupFailpoints {
    /// Returns a configuration with no injected failures.
    #[must_use]
    pub(crate) const fn none() -> Self {
        Self { before: 0, after: 0 }
    }

    /// Returns a configuration containing one failpoint.
    #[must_use]
    pub(crate) fn single(failpoint: StartupFailpoint) -> Self {
        Self::none().with(failpoint)
    }

    /// Enables one point without growing an unbounded collection.
    #[must_use]
    pub(crate) fn with(mut self, failpoint: StartupFailpoint) -> Self {
        let bit = phase_bit(failpoint.phase);
        match failpoint.boundary {
            StartupFailpointBoundary::Before => self.before |= bit,
            StartupFailpointBoundary::After => self.after |= bit,
        }
        self
    }

    /// Returns whether a point is enabled.
    #[must_use]
    pub(crate) fn contains(self, failpoint: StartupFailpoint) -> bool {
        let bit = phase_bit(failpoint.phase);
        match failpoint.boundary {
            StartupFailpointBoundary::Before => self.before & bit != 0,
            StartupFailpointBoundary::After => self.after & bit != 0,
        }
    }
}

fn phase_bit(phase: StartupPhase) -> u16 {
    1_u16 << phase_index(phase)
}
