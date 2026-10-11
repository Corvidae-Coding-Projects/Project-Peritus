//! Stable acceptance-contract validation failures.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use peritus_types::GateId;

#[doc = " Canonical collection whose ordering or uniqueness was invalid."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CanonicalCollection {
    #[doc = " Contract requirements."]
    Requirements,
    #[doc = " Contract exclusions."]
    Exclusions,
    #[doc = " Contract assumptions."]
    Assumptions,
    #[doc = " Gate definitions."]
    Gates,
    #[doc = " Dependencies within one gate."]
    GateDependencies,
    #[doc = " Evidence declarations attached to one gate."]
    GateEvidence,
    #[doc = " Required review categories."]
    ReviewCategories,
    #[doc = " Contract-wide evidence declarations."]
    EvidenceRequirements,
}
#[doc = " Bounded policy value that cannot be zero."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LimitKind {
    #[doc = " Maximum attempts for one gate."]
    GateAttempts,
    #[doc = " Maximum writer/reviewer/fixer cycles."]
    ReviewCycles,
    #[doc = " Required count of independent reviewers."]
    ReviewerQuorum,
    #[doc = " Gate execution timeout."]
    GateTimeout,
}
#[doc = " Stable machine-actionable class of specification failure."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SpecErrorKind {
    #[doc = " A required collection was empty."]
    EmptyCollection,
    #[doc = " A canonical collection repeated a value."]
    DuplicateCanonicalValue,
    #[doc = " A collection was not in strict canonical order."]
    NonCanonicalOrder,
    #[doc = " A gate names itself as a dependency."]
    SelfDependency,
    #[doc = " A gate names an undeclared dependency."]
    UnknownGateDependency,
    #[doc = " Gate dependencies contain a directed cycle."]
    GateCycle,
    #[doc = " A gate names undeclared required evidence."]
    UnknownEvidenceRequirement,
    #[doc = " An evidence declaration names a gate or category absent from the contract."]
    InvalidEvidenceSource,
    #[doc = " A required bounded policy value was zero."]
    ZeroLimit,
    #[doc = " A waiver declaration names evidence with the wrong source."]
    InvalidWaiverEvidence,
    #[doc = " Required final human-approval evidence is not declared."]
    MissingApprovalEvidence,
    #[doc = " Reviewer quorum cannot be reached within the maximum review cycles."]
    ReviewQuorumExceedsCycleLimit,
    #[doc = " The revision tuple is governed by another acceptance specification."]
    RevisionBindingMismatch,
}
#[doc = " Typed acceptance-contract validation failure."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SpecError {
    #[doc = " A required collection was empty."]
    EmptyCollection(CanonicalCollection),
    #[doc = " A canonical collection repeated a value."]
    DuplicateCanonicalValue(CanonicalCollection),
    #[doc = " A canonical collection was not strictly ordered."]
    NonCanonicalOrder(CanonicalCollection),
    #[doc = " A gate names itself as a dependency."]
    SelfDependency(GateId),
    #[doc = " A gate names an undeclared dependency."]
    UnknownGateDependency {
        #[doc = " Gate containing the invalid dependency edge."]
        gate: GateId,
        #[doc = " Undeclared dependency target."]
        dependency: GateId,
    },
    #[doc = " Gate dependencies contain a directed cycle."]
    GateCycle,
    #[doc = " A gate names undeclared required evidence."]
    UnknownEvidenceRequirement(GateId),
    #[doc = " An evidence declaration names a producer absent from the contract."]
    InvalidEvidenceSource(crate::EvidenceRequirementId),
    #[doc = " A required policy limit was zero."]
    ZeroLimit(LimitKind),
    #[doc = " A waiver names evidence that is absent or not waiver authorization."]
    InvalidWaiverEvidence,
    #[doc = " Final human approval is required but no matching evidence is declared."]
    MissingApprovalEvidence,
    #[doc = " The reviewer quorum exceeds the number of permitted review cycles."]
    ReviewQuorumExceedsCycleLimit {
        #[doc = " Required number of distinct review observations."]
        reviewer_quorum: u16,
        #[doc = " Maximum number of review cycles that may produce observations."]
        max_review_cycles: u16,
    },
    #[doc = " A revision tuple carries a different acceptance-specification identity."]
    RevisionBindingMismatch,
}
impl SpecError {
    #[doc = " Returns the stable error class."]
    #[must_use]
    pub const fn kind(&self) -> SpecErrorKind {
        match self {
            Self::EmptyCollection(_) => SpecErrorKind::EmptyCollection,
            Self::DuplicateCanonicalValue(_) => SpecErrorKind::DuplicateCanonicalValue,
            Self::NonCanonicalOrder(_) => SpecErrorKind::NonCanonicalOrder,
            Self::SelfDependency(_) => SpecErrorKind::SelfDependency,
            Self::UnknownGateDependency { .. } => SpecErrorKind::UnknownGateDependency,
            Self::GateCycle => SpecErrorKind::GateCycle,
            Self::UnknownEvidenceRequirement(_) => SpecErrorKind::UnknownEvidenceRequirement,
            Self::InvalidEvidenceSource(_) => SpecErrorKind::InvalidEvidenceSource,
            Self::ZeroLimit(_) => SpecErrorKind::ZeroLimit,
            Self::InvalidWaiverEvidence => SpecErrorKind::InvalidWaiverEvidence,
            Self::MissingApprovalEvidence => SpecErrorKind::MissingApprovalEvidence,
            Self::ReviewQuorumExceedsCycleLimit { .. } => {
                SpecErrorKind::ReviewQuorumExceedsCycleLimit
            }
            Self::RevisionBindingMismatch => SpecErrorKind::RevisionBindingMismatch,
        }
    }
    #[doc = " Returns the affected canonical collection, if applicable."]
    #[must_use]
    pub const fn collection(&self) -> Option<CanonicalCollection> {
        match self {
            Self::EmptyCollection(value)
            | Self::DuplicateCanonicalValue(value)
            | Self::NonCanonicalOrder(value) => Some(*value),
            _ => None,
        }
    }
}
