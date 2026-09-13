//! Exact-candidate observations for requirements, criteria, inventories, and artifacts.

use crate::{
    AcceptanceCriterion, EvidenceArtifactKind, IntegratedCandidate, InventoryKind,
    SecurityRequirement,
};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Fail-closed result of a required security control or criterion.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SecurityControlOutcome {
    /// The required behavior was observed directly and all assertions passed.
    Passed,
    /// Execution contradicted at least one required assertion.
    Failed,
    /// The control was not executed to a terminal observation.
    NotExecuted,
    /// The native subject lacked a required facility.
    Unsupported,
}

/// One aggregate R-SEC requirement observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequirementObservation {
    requirement: SecurityRequirement,
    candidate: IntegratedCandidate,
    outcome: SecurityControlOutcome,
    evidence_digest: Sha256Digest,
}

impl RequirementObservation {
    /// Creates one exact-candidate requirement result.
    #[must_use]
    pub const fn new(
        requirement: SecurityRequirement,
        candidate: IntegratedCandidate,
        outcome: SecurityControlOutcome,
        evidence_digest: Sha256Digest,
    ) -> Self {
        Self { requirement, candidate, outcome, evidence_digest }
    }

    /// Returns the literal R-SEC requirement.
    #[must_use]
    pub const fn requirement(&self) -> SecurityRequirement { self.requirement }

    /// Returns the exact observed candidate.
    #[must_use]
    pub const fn candidate(&self) -> (result: IntegratedCandidate)
        ensures result == self.spec_candidate(),
    {
        self.candidate
    }

    /// Returns the terminal control outcome.
    #[must_use]
    pub const fn outcome(&self) -> SecurityControlOutcome { self.outcome }

    /// Returns the digest of supporting native evidence.
    #[must_use]
    pub const fn evidence_digest(&self) -> Sha256Digest { self.evidence_digest }

    /// Specification view of the observed candidate.
    pub closed spec fn spec_candidate(&self) -> IntegratedCandidate { self.candidate }
}

/// One aggregate numbered acceptance-criterion observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CriterionObservation {
    criterion: AcceptanceCriterion,
    candidate: IntegratedCandidate,
    outcome: SecurityControlOutcome,
    evidence_digest: Sha256Digest,
}

impl CriterionObservation {
    /// Creates one exact-candidate numbered criterion result.
    #[must_use]
    pub const fn new(
        criterion: AcceptanceCriterion,
        candidate: IntegratedCandidate,
        outcome: SecurityControlOutcome,
        evidence_digest: Sha256Digest,
    ) -> Self {
        Self { criterion, candidate, outcome, evidence_digest }
    }

    /// Returns the authoritative criterion identity.
    #[must_use]
    pub const fn criterion(&self) -> AcceptanceCriterion { self.criterion }

    /// Returns the exact observed candidate.
    #[must_use]
    pub const fn candidate(&self) -> (result: IntegratedCandidate)
        ensures result == self.spec_candidate(),
    {
        self.candidate
    }

    /// Returns the terminal criterion outcome.
    #[must_use]
    pub const fn outcome(&self) -> SecurityControlOutcome { self.outcome }

    /// Returns the digest of supporting native evidence.
    #[must_use]
    pub const fn evidence_digest(&self) -> Sha256Digest { self.evidence_digest }

    /// Specification view of the observed candidate.
    pub closed spec fn spec_candidate(&self) -> IntegratedCandidate { self.candidate }
}

/// One reviewed inventory observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InventoryObservation {
    kind: InventoryKind,
    candidate: IntegratedCandidate,
    complete: bool,
    evidence_digest: Sha256Digest,
}

impl InventoryObservation {
    /// Creates one exact-candidate inventory observation.
    #[must_use]
    pub const fn new(
        kind: InventoryKind,
        candidate: IntegratedCandidate,
        complete: bool,
        evidence_digest: Sha256Digest,
    ) -> Self {
        Self { kind, candidate, complete, evidence_digest }
    }

    /// Returns the inventory role.
    #[must_use]
    pub const fn kind(&self) -> InventoryKind { self.kind }

    /// Returns the exact observed candidate.
    #[must_use]
    pub const fn candidate(&self) -> (result: IntegratedCandidate)
        ensures result == self.spec_candidate(),
    {
        self.candidate
    }

    /// Reports whether reconciliation found the inventory complete.
    #[must_use]
    pub const fn complete(&self) -> bool { self.complete }

    /// Returns the reviewed inventory digest.
    #[must_use]
    pub const fn evidence_digest(&self) -> Sha256Digest { self.evidence_digest }

    /// Specification view of the observed candidate.
    pub closed spec fn spec_candidate(&self) -> IntegratedCandidate { self.candidate }
}

/// One canonical evidence-manifest role observation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArtifactObservation {
    kind: EvidenceArtifactKind,
    candidate: IntegratedCandidate,
    digest: Sha256Digest,
}

impl ArtifactObservation {
    /// Creates one exact-candidate evidence artifact reference.
    #[must_use]
    pub const fn new(
        kind: EvidenceArtifactKind,
        candidate: IntegratedCandidate,
        digest: Sha256Digest,
    ) -> Self {
        Self { kind, candidate, digest }
    }

    /// Returns the manifest role.
    #[must_use]
    pub const fn kind(&self) -> EvidenceArtifactKind { self.kind }

    /// Returns the exact candidate to which the artifact is bound.
    #[must_use]
    pub const fn candidate(&self) -> (result: IntegratedCandidate)
        ensures result == self.spec_candidate(),
    {
        self.candidate
    }

    /// Returns the exact artifact digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest { self.digest }

    /// Specification view of the observed candidate.
    pub closed spec fn spec_candidate(&self) -> IntegratedCandidate { self.candidate }
}

} // verus!
