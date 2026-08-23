//! Stable acceptance-contract validation failures.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use peritus_types::GateId;
use vstd::prelude::*;

verus! {

/// Canonical collection whose ordering or uniqueness was invalid.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CanonicalCollection {
    /// Contract requirements.
    Requirements,
    /// Contract exclusions.
    Exclusions,
    /// Contract assumptions.
    Assumptions,
    /// Gate definitions.
    Gates,
    /// Dependencies within one gate.
    GateDependencies,
    /// Evidence declarations attached to one gate.
    GateEvidence,
    /// Required review categories.
    ReviewCategories,
    /// Contract-wide evidence declarations.
    EvidenceRequirements,
}

/// Bounded policy value that cannot be zero.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LimitKind {
    /// Maximum attempts for one gate.
    GateAttempts,
    /// Maximum writer/reviewer/fixer cycles.
    ReviewCycles,
    /// Required count of independent reviewers.
    ReviewerQuorum,
    /// Gate execution timeout.
    GateTimeout,
}

/// Stable machine-actionable class of specification failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SpecErrorKind {
    /// A required collection was empty.
    EmptyCollection,
    /// A canonical collection repeated a value.
    DuplicateCanonicalValue,
    /// A collection was not in strict canonical order.
    NonCanonicalOrder,
    /// A gate names itself as a dependency.
    SelfDependency,
    /// A gate names an undeclared dependency.
    UnknownGateDependency,
    /// Gate dependencies contain a directed cycle.
    GateCycle,
    /// A gate names undeclared required evidence.
    UnknownEvidenceRequirement,
    /// An evidence declaration names a gate or category absent from the contract.
    InvalidEvidenceSource,
    /// A required bounded policy value was zero.
    ZeroLimit,
    /// A waiver declaration names evidence with the wrong source.
    InvalidWaiverEvidence,
    /// Required final human-approval evidence is not declared.
    MissingApprovalEvidence,
    /// Reviewer quorum cannot be reached within the maximum review cycles.
    ReviewQuorumExceedsCycleLimit,
    /// The revision tuple is governed by another acceptance specification.
    RevisionBindingMismatch,
}

/// Typed acceptance-contract validation failure.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SpecError {
    /// A required collection was empty.
    EmptyCollection(CanonicalCollection),
    /// A canonical collection repeated a value.
    DuplicateCanonicalValue(CanonicalCollection),
    /// A canonical collection was not strictly ordered.
    NonCanonicalOrder(CanonicalCollection),
    /// A gate names itself as a dependency.
    SelfDependency(GateId),
    /// A gate names an undeclared dependency.
    UnknownGateDependency {
        /// Gate containing the invalid dependency edge.
        gate: GateId,
        /// Undeclared dependency target.
        dependency: GateId,
    },
    /// Gate dependencies contain a directed cycle.
    GateCycle,
    /// A gate names undeclared required evidence.
    UnknownEvidenceRequirement(GateId),
    /// An evidence declaration names a producer absent from the contract.
    InvalidEvidenceSource(crate::EvidenceRequirementId),
    /// A required policy limit was zero.
    ZeroLimit(LimitKind),
    /// A waiver names evidence that is absent or not waiver authorization.
    InvalidWaiverEvidence,
    /// Final human approval is required but no matching evidence is declared.
    MissingApprovalEvidence,
    /// The reviewer quorum exceeds the number of permitted review cycles.
    ReviewQuorumExceedsCycleLimit {
        /// Required number of distinct review observations.
        reviewer_quorum: u16,
        /// Maximum number of review cycles that may produce observations.
        max_review_cycles: u16,
    },
    /// A revision tuple carries a different acceptance-specification identity.
    RevisionBindingMismatch,
}

impl SpecError {
    /// Returns the stable error class.
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

    /// Returns the affected canonical collection, if applicable.
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

} // verus!
