//! D1 gate observations with failure-cause conservation.

use peritus_gates::{GateOutcomeKind, GateRunPhase, GateRunState, GateTerminalKind};
use peritus_types::{RevisionTuple, RunId, Sha256Digest};

use super::{ChildAggregateKind, ChildHead, ChildTerminalClass, binding, stale};
use crate::{OrchestratorError, QualityCycleBinding};

/// Failure-safe classification of a terminal D1 run.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum GateObservationClass {
    /// Every required gate passed with current evidence.
    Passed,
    /// Only authoritative frozen-predicate failures prevented success.
    CandidateFailed,
    /// Infrastructure or incomplete evidence prevented a trustworthy pass.
    InfrastructureFailed,
    /// Cancellation completed without success.
    Cancelled,
    /// Recovery could not establish a trustworthy outcome.
    Indeterminate,
}

/// Checked terminal D1 projection retaining exact plan and evidence bindings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateChildObservation {
    orchestrator_run_id: RunId,
    gate_run_id: RunId,
    revision: RevisionTuple,
    plan_digest: Sha256Digest,
    snapshot_digest: Sha256Digest,
    evidence_digest: Sha256Digest,
    class: GateObservationClass,
    head: ChildHead,
}

/// Canonical decoded D1 observation fields used by the family codecs.
#[doc(hidden)]
pub struct GateObservationWire {
    /// Overall E0 run identity.
    pub orchestrator_run_id: RunId,
    /// Exact D1 child run identity.
    pub gate_run_id: RunId,
    /// Candidate revision evaluated by D1.
    pub revision: RevisionTuple,
    /// Canonical D1 plan digest.
    pub plan_digest: Sha256Digest,
    /// Exact clean snapshot evaluated by D1.
    pub snapshot_digest: Sha256Digest,
    /// Terminal gate evidence digest.
    pub evidence_digest: Sha256Digest,
    /// Normalized gate outcome.
    pub class: GateObservationClass,
    /// Authoritative terminal D1 head.
    pub head: ChildHead,
}

impl GateChildObservation {
    /// Projects one terminal D1 state and preserves candidate-vs-infrastructure truth.
    ///
    /// # Errors
    ///
    /// Returns an error when the D1 state is nonterminal or differs from the current cycle.
    pub fn from_state(
        state: &GateRunState,
        orchestrator_run_id: RunId,
        cycle: &QualityCycleBinding,
    ) -> Result<Self, OrchestratorError> {
        let run_matches = state.run_id() == cycle.gate_run_id();
        let revision_matches = state.revision() == cycle.revision();
        let plan_matches = state.plan_digest() == cycle.gate_plan_digest();
        if !(run_matches && revision_matches && plan_matches) {
            return Err(binding("D1 state differs from the current quality-cycle binding"));
        }
        if state.phase() != GateRunPhase::Terminal {
            return Err(stale("D1 observation is not terminal"));
        }
        let terminal =
            state.terminal().ok_or_else(|| binding("terminal D1 state lacks summary"))?;
        let (class, normalized) = match terminal.kind() {
            GateTerminalKind::Passed => {
                (GateObservationClass::Passed, ChildTerminalClass::Completed)
            }
            GateTerminalKind::Cancelled => {
                (GateObservationClass::Cancelled, ChildTerminalClass::Cancelled)
            }
            GateTerminalKind::Indeterminate => {
                (GateObservationClass::Indeterminate, ChildTerminalClass::Indeterminate)
            }
            GateTerminalKind::Failed if candidate_failure_only(state) => {
                (GateObservationClass::CandidateFailed, ChildTerminalClass::Failed)
            }
            GateTerminalKind::Failed => {
                (GateObservationClass::InfrastructureFailed, ChildTerminalClass::Failed)
            }
        };
        let head = ChildHead::new(
            ChildAggregateKind::Gates,
            state.sequence(),
            state.last_event_id(),
            state.state_digest(),
            Some(normalized),
        )?;
        Ok(Self {
            orchestrator_run_id,
            gate_run_id: state.run_id(),
            revision: state.revision(),
            plan_digest: state.plan_digest(),
            snapshot_digest: state.snapshot_digest(),
            evidence_digest: terminal.digest(),
            class,
            head,
        })
    }

    pub(crate) fn from_wire(value: &GateObservationWire) -> Result<Self, OrchestratorError> {
        let GateObservationWire {
            orchestrator_run_id,
            gate_run_id,
            revision,
            plan_digest,
            snapshot_digest,
            evidence_digest,
            class,
            head,
        } = *value;
        if head.aggregate() != ChildAggregateKind::Gates || Some(terminal(class)) != head.terminal()
        {
            return Err(binding("decoded D1 observation is inconsistent"));
        }
        Ok(Self {
            orchestrator_run_id,
            gate_run_id,
            revision,
            plan_digest,
            snapshot_digest,
            evidence_digest,
            class,
            head,
        })
    }

    #[must_use]
    /// Returns the overall E0 run identity.
    pub const fn run_id(&self) -> RunId {
        self.orchestrator_run_id
    }
    #[must_use]
    /// Returns the exact D1 child run identity.
    pub const fn gate_run_id(&self) -> RunId {
        self.gate_run_id
    }
    #[must_use]
    /// Returns the candidate revision evaluated by D1.
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[must_use]
    /// Returns the exact canonical gate-plan digest.
    pub const fn plan_digest(&self) -> Sha256Digest {
        self.plan_digest
    }
    /// Returns the exact clean snapshot evaluated by D1.
    #[must_use]
    pub const fn snapshot_digest(&self) -> Sha256Digest {
        self.snapshot_digest
    }
    #[must_use]
    /// Returns the terminal gate evidence digest.
    pub const fn evidence_digest(&self) -> Sha256Digest {
        self.evidence_digest
    }
    #[must_use]
    /// Returns the normalized gate outcome classification.
    pub const fn class(&self) -> GateObservationClass {
        self.class
    }
    #[must_use]
    /// Returns the authoritative terminal D1 head.
    pub const fn head(&self) -> ChildHead {
        self.head
    }
}

fn candidate_failure_only(state: &GateRunState) -> bool {
    let mut candidate = false;
    for slot in state.slots() {
        if let Some(result) = slot.last_result() {
            if result.kind() == GateOutcomeKind::CandidateFailure {
                candidate = true;
            } else if !result.passed() {
                return false;
            }
        }
    }
    candidate
}

const fn terminal(class: GateObservationClass) -> ChildTerminalClass {
    match class {
        GateObservationClass::Passed => ChildTerminalClass::Completed,
        GateObservationClass::CandidateFailed | GateObservationClass::InfrastructureFailed => {
            ChildTerminalClass::Failed
        }
        GateObservationClass::Cancelled => ChildTerminalClass::Cancelled,
        GateObservationClass::Indeterminate => ChildTerminalClass::Indeterminate,
    }
}
