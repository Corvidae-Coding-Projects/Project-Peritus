//! Rebuildable read-only D1 projections.

use peritus_types::{GateId, RevisionTuple, RunId, Sha256Digest};

use crate::{GateRunPhase, GateRunState, GateSlotPhase, GateTerminalKind};

/// One canonical per-gate projection row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectedGate {
    gate_id: GateId,
    phase: GateSlotPhase,
    attempts: u16,
    blocked_by: Option<GateId>,
    result_digest: Option<Sha256Digest>,
    evidence_digest: Option<Sha256Digest>,
}

impl ProjectedGate {
    /// Returns the gate identity.
    #[must_use]
    pub const fn gate_id(&self) -> GateId {
        self.gate_id
    }
    /// Returns the authoritative projected phase.
    #[must_use]
    pub const fn phase(&self) -> GateSlotPhase {
        self.phase
    }
    /// Returns attempts prepared so far.
    #[must_use]
    pub const fn attempts(&self) -> u16 {
        self.attempts
    }
    /// Returns the first canonical failed prerequisite.
    #[must_use]
    pub const fn blocked_by(&self) -> Option<GateId> {
        self.blocked_by
    }
    /// Returns the latest complete C4 terminal digest.
    #[must_use]
    pub const fn result_digest(&self) -> Option<Sha256Digest> {
        self.result_digest
    }
    /// Returns the exact evidence receipt digest.
    #[must_use]
    pub const fn evidence_digest(&self) -> Option<Sha256Digest> {
        self.evidence_digest
    }
}

/// Canonical run summary rebuilt solely from authoritative state/events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectedRun {
    run_id: RunId,
    revision: RevisionTuple,
    phase: GateRunPhase,
    terminal: Option<GateTerminalKind>,
    sequence: u64,
    state_digest: Sha256Digest,
}

impl ProjectedRun {
    /// Returns the run identity.
    #[must_use]
    pub const fn run_id(&self) -> RunId {
        self.run_id
    }
    /// Returns the exact revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the run lifecycle.
    #[must_use]
    pub const fn phase(&self) -> GateRunPhase {
        self.phase
    }
    /// Returns the terminal kind when finalized.
    #[must_use]
    pub const fn terminal(&self) -> Option<GateTerminalKind> {
        self.terminal
    }
    /// Returns the latest aggregate sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Returns the authoritative state digest.
    #[must_use]
    pub const fn state_digest(&self) -> Sha256Digest {
        self.state_digest
    }
}

/// Complete rebuildable projection with no dispatch or acceptance authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateProjection {
    run: ProjectedRun,
    gates: Vec<ProjectedGate>,
}

impl GateProjection {
    /// Projects one checked authoritative state deterministically.
    #[must_use]
    pub fn from_state(state: &GateRunState) -> Self {
        let run = ProjectedRun {
            run_id: state.run_id(),
            revision: state.revision(),
            phase: state.phase(),
            terminal: state.terminal().map(crate::GateTerminal::kind),
            sequence: state.sequence().get(),
            state_digest: state.state_digest(),
        };
        let gates = state
            .slots()
            .iter()
            .map(|slot| ProjectedGate {
                gate_id: slot.gate_id(),
                phase: slot.phase(),
                attempts: slot.attempts(),
                blocked_by: slot.blocked_by(),
                result_digest: slot.last_result().map(crate::GateAttemptResult::tool_result_digest),
                evidence_digest: slot.evidence().map(crate::GateEvidenceReceipt::receipt_digest),
            })
            .collect();
        Self { run, gates }
    }

    /// Borrows the run summary.
    #[must_use]
    pub const fn run(&self) -> &ProjectedRun {
        &self.run
    }
    /// Borrows canonical gate rows.
    #[must_use]
    pub fn gates(&self) -> &[ProjectedGate] {
        &self.gates
    }
}
