//! Exact-revision deterministic gate observations.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::GateAttemptOrdinal;
use peritus_types::{GateExecutionId, GateId, RevisionTuple, Sha256Digest};
use vstd::prelude::*;

verus! {

/// Normalized reason a deterministic gate did not pass.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GateFailure {
    /// The gate ran successfully but its success predicate was false.
    PredicateFailed,
    /// The gate process reported an unsuccessful exit.
    UnsuccessfulExit,
    /// The gate could not produce a valid normalized result.
    InvalidResult,
    /// Infrastructure prevented a trustworthy gate result.
    Infrastructure,
}

/// Normalized gate result.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum GateOutcome {
    /// The frozen gate success rule passed.
    Passed,
    /// The gate did not pass for the stated reason.
    Failed(GateFailure),
}

/// One gate execution result bound to the complete revision tuple.
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
    /// Specification view of the exact observed revision.
    pub closed spec fn spec_revision(&self) -> RevisionTuple { self.revision }

    /// Specification view of the observed result.
    pub closed spec fn spec_outcome(&self) -> GateOutcome { self.outcome }

    /// Specification view of the one-based execution attempt.
    pub closed spec fn spec_attempt(&self) -> u16 { self.attempt.spec_value() }

    /// Creates a normalized gate observation.
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

    /// Returns the immutable execution identity.
    #[must_use]
    pub const fn execution_id(&self) -> GateExecutionId { self.execution_id }

    /// Returns the gate identity declared by the contract.
    #[must_use]
    pub const fn gate_id(&self) -> GateId { self.gate_id }

    /// Returns the one-based attempt number for this gate execution.
    #[must_use]
    pub const fn attempt(&self) -> (attempt: GateAttemptOrdinal)
        ensures attempt.spec_value() == self.spec_attempt()
    { self.attempt }

    /// Returns the exact revision observed by the gate.
    #[must_use]
    pub const fn revision(&self) -> (revision: RevisionTuple)
        ensures revision == self.spec_revision()
    { self.revision }

    /// Returns the normalized result.
    #[must_use]
    pub const fn outcome(&self) -> (outcome: GateOutcome)
        ensures outcome == self.spec_outcome()
    { self.outcome }

    /// Returns whether the normalized result passed.
    #[must_use]
    pub const fn passed(&self) -> (passed: bool)
        ensures passed == (self.spec_outcome() == GateOutcome::Passed)
    { matches!(self.outcome, GateOutcome::Passed) }

    /// Returns the digest of the normalized gate result.
    #[must_use]
    pub const fn result_digest(&self) -> Sha256Digest { self.result_digest }
}

} // verus!
