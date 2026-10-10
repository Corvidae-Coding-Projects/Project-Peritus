//! Exact-candidate observations for requirements, criteria, inventories, and artifacts.

use crate::{
    AcceptanceCriterion, EvidenceArtifactKind, IntegratedCandidate, InventoryKind,
    SecurityRequirement,
};
use peritus_types::Sha256Digest;

#[doc = " Fail-closed result of a required security control or criterion."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SecurityControlOutcome {
    #[doc = " The required behavior was observed directly and all assertions passed."]
    Passed,
    #[doc = " Execution contradicted at least one required assertion."]
    Failed,
    #[doc = " The control was not executed to a terminal observation."]
    NotExecuted,
    #[doc = " The native subject lacked a required facility."]
    Unsupported,
}
impl SecurityControlOutcome {
    #[doc = " Reports whether this observation reached the sole passing terminal state."]
    #[must_use]
    pub const fn is_passed(self) -> bool {
        matches!(self, Self::Passed)
    }
}
#[doc = " One aggregate R-SEC requirement observation."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequirementObservation {
    requirement: SecurityRequirement,
    candidate: IntegratedCandidate,
    outcome: SecurityControlOutcome,
    evidence_digest: Sha256Digest,
}
impl RequirementObservation {
    #[doc = " Creates one exact-candidate requirement result."]
    #[must_use]
    pub const fn new(
        requirement: SecurityRequirement,
        candidate: IntegratedCandidate,
        outcome: SecurityControlOutcome,
        evidence_digest: Sha256Digest,
    ) -> Self {
        Self { requirement, candidate, outcome, evidence_digest }
    }
    #[doc = " Returns the literal R-SEC requirement."]
    #[must_use]
    pub const fn requirement(&self) -> SecurityRequirement {
        self.requirement
    }
    #[doc = " Returns the exact observed candidate."]
    #[must_use]
    pub const fn candidate(&self) -> IntegratedCandidate {
        self.candidate
    }
    #[doc = " Returns the terminal control outcome."]
    #[must_use]
    pub const fn outcome(&self) -> SecurityControlOutcome {
        self.outcome
    }
    #[doc = " Returns the digest of supporting native evidence."]
    #[must_use]
    pub const fn evidence_digest(&self) -> Sha256Digest {
        self.evidence_digest
    }
}
#[doc = " One aggregate numbered acceptance-criterion observation."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CriterionObservation {
    criterion: AcceptanceCriterion,
    candidate: IntegratedCandidate,
    outcome: SecurityControlOutcome,
    evidence_digest: Sha256Digest,
}
impl CriterionObservation {
    #[doc = " Creates one exact-candidate numbered criterion result."]
    #[must_use]
    pub const fn new(
        criterion: AcceptanceCriterion,
        candidate: IntegratedCandidate,
        outcome: SecurityControlOutcome,
        evidence_digest: Sha256Digest,
    ) -> Self {
        Self { criterion, candidate, outcome, evidence_digest }
    }
    #[doc = " Returns the authoritative criterion identity."]
    #[must_use]
    pub const fn criterion(&self) -> AcceptanceCriterion {
        self.criterion
    }
    #[doc = " Returns the exact observed candidate."]
    #[must_use]
    pub const fn candidate(&self) -> IntegratedCandidate {
        self.candidate
    }
    #[doc = " Returns the terminal criterion outcome."]
    #[must_use]
    pub const fn outcome(&self) -> SecurityControlOutcome {
        self.outcome
    }
    #[doc = " Returns the digest of supporting native evidence."]
    #[must_use]
    pub const fn evidence_digest(&self) -> Sha256Digest {
        self.evidence_digest
    }
}
#[doc = " One reviewed inventory observation."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InventoryObservation {
    kind: InventoryKind,
    candidate: IntegratedCandidate,
    complete: bool,
    evidence_digest: Sha256Digest,
}
impl InventoryObservation {
    #[doc = " Creates one exact-candidate inventory observation."]
    #[must_use]
    pub const fn new(
        kind: InventoryKind,
        candidate: IntegratedCandidate,
        complete: bool,
        evidence_digest: Sha256Digest,
    ) -> Self {
        Self { kind, candidate, complete, evidence_digest }
    }
    #[doc = " Returns the inventory role."]
    #[must_use]
    pub const fn kind(&self) -> InventoryKind {
        self.kind
    }
    #[doc = " Returns the exact observed candidate."]
    #[must_use]
    pub const fn candidate(&self) -> IntegratedCandidate {
        self.candidate
    }
    #[doc = " Reports whether reconciliation found the inventory complete."]
    #[must_use]
    pub const fn complete(&self) -> bool {
        self.complete
    }
    #[doc = " Returns the reviewed inventory digest."]
    #[must_use]
    pub const fn evidence_digest(&self) -> Sha256Digest {
        self.evidence_digest
    }
}
#[doc = " One canonical evidence-manifest role observation."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArtifactObservation {
    kind: EvidenceArtifactKind,
    candidate: IntegratedCandidate,
    digest: Sha256Digest,
}
impl ArtifactObservation {
    #[doc = " Creates one exact-candidate evidence artifact reference."]
    #[must_use]
    pub const fn new(
        kind: EvidenceArtifactKind,
        candidate: IntegratedCandidate,
        digest: Sha256Digest,
    ) -> Self {
        Self { kind, candidate, digest }
    }
    #[doc = " Returns the manifest role."]
    #[must_use]
    pub const fn kind(&self) -> EvidenceArtifactKind {
        self.kind
    }
    #[doc = " Returns the exact candidate to which the artifact is bound."]
    #[must_use]
    pub const fn candidate(&self) -> IntegratedCandidate {
        self.candidate
    }
    #[doc = " Returns the exact artifact digest."]
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}
