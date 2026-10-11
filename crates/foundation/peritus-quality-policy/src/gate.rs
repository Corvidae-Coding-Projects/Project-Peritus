//! Exact-revision deterministic gate observations.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::GateAttemptOrdinal;
use peritus_types::{GateExecutionId, GateId, RevisionTuple, Sha256Digest};

#[doc = " Normalized reason a deterministic gate did not pass."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GateFailure {
    #[doc = " The gate ran successfully but its success predicate was false."]
    PredicateFailed,
    #[doc = " The gate process reported an unsuccessful exit."]
    UnsuccessfulExit,
    #[doc = " The gate could not produce a valid normalized result."]
    InvalidResult,
    #[doc = " Infrastructure prevented a trustworthy gate result."]
    Infrastructure,
}
#[doc = " Normalized gate result."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GateOutcome {
    #[doc = " The frozen gate success rule passed."]
    Passed,
    #[doc = " The gate did not pass for the stated reason."]
    Failed(GateFailure),
}
#[doc = " One gate execution result bound to the complete revision tuple."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GateObservation {
    execution_id: GateExecutionId,
    gate_id: GateId,
    attempt: GateAttemptOrdinal,
    revision: RevisionTuple,
    outcome: GateOutcome,
    result_digest: Sha256Digest,
}
impl GateObservation {
    #[doc = " Creates a normalized gate observation."]
    #[must_use]
    pub const fn new(
        execution_id: GateExecutionId,
        gate_id: GateId,
        attempt: GateAttemptOrdinal,
        revision: RevisionTuple,
        outcome: GateOutcome,
        result_digest: Sha256Digest,
    ) -> Self {
        Self { execution_id, gate_id, attempt, revision, outcome, result_digest }
    }
    #[doc = " Returns the immutable execution identity."]
    #[must_use]
    pub const fn execution_id(&self) -> GateExecutionId {
        self.execution_id
    }
    #[doc = " Returns the gate identity declared by the contract."]
    #[must_use]
    pub const fn gate_id(&self) -> GateId {
        self.gate_id
    }
    #[doc = " Returns the one-based attempt number for this gate execution."]
    #[must_use]
    pub const fn attempt(&self) -> GateAttemptOrdinal {
        self.attempt
    }
    #[doc = " Returns the exact revision observed by the gate."]
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns the normalized result."]
    #[must_use]
    pub const fn outcome(&self) -> GateOutcome {
        self.outcome
    }
    #[doc = " Returns whether the normalized result passed."]
    #[must_use]
    pub const fn passed(&self) -> bool {
        matches!(self.outcome, GateOutcome::Passed)
    }
    #[doc = " Returns the digest of the normalized gate result."]
    #[must_use]
    pub const fn result_digest(&self) -> Sha256Digest {
        self.result_digest
    }
}
