//! Deterministic production-lifecycle fixtures for external resilience qualification.

mod certificate;
mod domain;
mod journey;
mod scenario;

use crate::{ActivePhase, OrchestratorCommand, OrchestratorState, OrchestratorTransition};

/// One active E0 phase covered by the H1 daemon-restart matrix.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LifecyclePhase {
    /// A writer handoff exists but has not started.
    WriterPending,
    /// Writer work has durable D3 ownership.
    WriterActive,
    /// The writer candidate awaits gates.
    GatesPending,
    /// Gates own the current candidate.
    GatesActive,
    /// A reviewer handoff exists but has not started.
    ReviewPending,
    /// Reviewer work has durable D3 ownership.
    ReviewActive,
    /// A fixer handoff exists but has not started.
    FixerPending,
    /// Fixer work has durable D3 ownership.
    FixerActive,
    /// A checked fixer proposal awaits revision advancement.
    RevisionAdvancing,
    /// Successful quality evidence awaits B2 evaluation.
    EvaluatingAcceptance,
    /// A B2 certificate awaits durable B0 acceptance.
    KernelAcceptancePending,
}

impl LifecyclePhase {
    /// Parses the stable H1 catalog phase code.
    #[must_use]
    pub const fn parse(value: &str) -> Option<Self> {
        match value.as_bytes() {
            b"writer-pending" => Some(Self::WriterPending),
            b"writer-active" => Some(Self::WriterActive),
            b"gates-pending" => Some(Self::GatesPending),
            b"gates-active" => Some(Self::GatesActive),
            b"review-pending" => Some(Self::ReviewPending),
            b"review-active" => Some(Self::ReviewActive),
            b"fixer-pending" => Some(Self::FixerPending),
            b"fixer-active" => Some(Self::FixerActive),
            b"revision-advancing" => Some(Self::RevisionAdvancing),
            b"evaluating-acceptance" => Some(Self::EvaluatingAcceptance),
            b"kernel-acceptance-pending" => Some(Self::KernelAcceptancePending),
            _ => None,
        }
    }

    /// Returns the stable H1 catalog phase code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::WriterPending => "writer-pending",
            Self::WriterActive => "writer-active",
            Self::GatesPending => "gates-pending",
            Self::GatesActive => "gates-active",
            Self::ReviewPending => "review-pending",
            Self::ReviewActive => "review-active",
            Self::FixerPending => "fixer-pending",
            Self::FixerActive => "fixer-active",
            Self::RevisionAdvancing => "revision-advancing",
            Self::EvaluatingAcceptance => "evaluating-acceptance",
            Self::KernelAcceptancePending => "kernel-acceptance-pending",
        }
    }

    const fn active(self) -> ActivePhase {
        match self {
            Self::WriterPending => ActivePhase::WriterPending,
            Self::WriterActive => ActivePhase::WriterActive,
            Self::GatesPending => ActivePhase::GatesPending,
            Self::GatesActive => ActivePhase::GatesActive,
            Self::ReviewPending => ActivePhase::ReviewPending,
            Self::ReviewActive => ActivePhase::ReviewActive,
            Self::FixerPending => ActivePhase::FixerPending,
            Self::FixerActive => ActivePhase::FixerActive,
            Self::RevisionAdvancing => ActivePhase::RevisionAdvancing,
            Self::EvaluatingAcceptance => ActivePhase::EvaluatingAcceptance,
            Self::KernelAcceptancePending => ActivePhase::KernelAcceptancePending,
        }
    }
}

/// A reducer-produced command/transition prefix ending at one exact active phase.
pub struct LifecycleFixture {
    phase: LifecyclePhase,
    steps: Vec<(OrchestratorCommand, OrchestratorTransition)>,
    state: OrchestratorState,
}

impl LifecycleFixture {
    /// Builds the shortest legal reducer path to `phase` from a canonical genesis command.
    ///
    /// # Errors
    ///
    /// Returns an error if the pinned genesis corpus is invalid or a production transition rejects
    /// the deterministic qualification journey.
    pub fn build(phase: LifecyclePhase) -> Result<Self, &'static str> {
        let scenario = journey::to_phase(phase)?;
        if scenario.state().phase() != crate::OrchestratorPhase::Active(phase.active()) {
            return Err("lifecycle fixture ended at another orchestrator phase");
        }
        let state = scenario.state().clone();
        Ok(Self { phase, steps: scenario.into_steps(), state })
    }

    /// Returns the exact requested active phase.
    #[must_use]
    pub const fn phase(&self) -> LifecyclePhase {
        self.phase
    }

    /// Returns the complete ordered command/transition prefix for atomic C0 commits.
    #[must_use]
    pub fn steps(&self) -> &[(OrchestratorCommand, OrchestratorTransition)] {
        &self.steps
    }

    /// Returns the exact terminal checkpoint state in this prefix.
    #[must_use]
    pub const fn state(&self) -> &OrchestratorState {
        &self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_h1_phase_is_reducer_reachable_and_replay_exact() {
        for phase in [
            LifecyclePhase::WriterPending,
            LifecyclePhase::WriterActive,
            LifecyclePhase::GatesPending,
            LifecyclePhase::GatesActive,
            LifecyclePhase::ReviewPending,
            LifecyclePhase::ReviewActive,
            LifecyclePhase::FixerPending,
            LifecyclePhase::FixerActive,
            LifecyclePhase::RevisionAdvancing,
            LifecyclePhase::EvaluatingAcceptance,
            LifecyclePhase::KernelAcceptancePending,
        ] {
            let fixture = LifecycleFixture::build(phase).expect("build lifecycle fixture");
            let events = fixture
                .steps()
                .iter()
                .map(|(_, transition)| transition.event().clone())
                .collect::<Vec<_>>();
            let replayed = crate::replay(&events).expect("replay lifecycle prefix");
            assert_eq!(replayed, fixture.state().clone());
        }
    }
}
