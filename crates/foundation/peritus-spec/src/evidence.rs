//! Required evidence and explicit human-authority declarations.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{ContentReference, EvidenceRequirementId, ReviewCategory};
use peritus_types::GateId;
use vstd::prelude::*;

verus! {

/// Contract-declared producer class for required evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvidenceSource {
    /// Evidence not owned by a gate, review category, approval, or waiver.
    General,
    /// Evidence produced by one declared gate.
    Gate(GateId),
    /// Evidence produced by a review of one declared category.
    Review(ReviewCategory),
    /// Evidence of explicit human approval.
    HumanApproval,
    /// Evidence authorizing a blocker waiver.
    WaiverAuthorization,
}

/// Export handling declared for an evidence item.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExportClassification {
    /// Safe for the ordinary evidence bundle.
    Public,
    /// Included only in controlled project-internal exports.
    Internal,
    /// Included only in separately authorized restricted exports.
    Restricted,
}

/// One required evidence item and its immutable meaning.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvidenceRequirement {
    id: EvidenceRequirementId,
    description: ContentReference,
    source: EvidenceSource,
    export: ExportClassification,
}

impl EvidenceRequirement {
    /// Creates a required evidence declaration.
    #[must_use]
    pub const fn new(
        id: EvidenceRequirementId,
        description: ContentReference,
        source: EvidenceSource,
        export: ExportClassification,
    ) -> Self {
        Self { id, description, source, export }
    }

    /// Returns the stable evidence requirement identifier.
    #[must_use]
    pub const fn id(&self) -> EvidenceRequirementId { self.id }

    /// Returns the immutable description reference.
    #[must_use]
    pub const fn description(&self) -> ContentReference { self.description }

    /// Returns the declared evidence producer class.
    #[must_use]
    pub const fn source(&self) -> EvidenceSource { self.source }

    /// Returns the declared export handling.
    #[must_use]
    pub const fn export_classification(&self) -> ExportClassification { self.export }
}

/// Explicit declaration of whether final human approval is required.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HumanApprovalPolicy {
    /// The contract does not require final human approval.
    NotRequired,
    /// Acceptance requires approval under the referenced immutable policy.
    Required(ContentReference),
}

impl HumanApprovalPolicy {
    /// Returns whether an explicit approval observation is required.
    #[must_use]
    pub const fn is_required(&self) -> bool { matches!(self, Self::Required(_)) }

    /// Returns the authority policy reference when approval is required.
    #[must_use]
    pub const fn authority(&self) -> Option<ContentReference> {
        match self { Self::NotRequired => None, Self::Required(reference) => Some(*reference) }
    }
}

/// Explicit declaration governing blocker waivers.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WaiverPolicy {
    /// Blockers cannot be waived under this contract.
    Forbidden,
    /// Waivers require the named authority policy and evidence declaration.
    Allowed {
        /// Immutable policy describing who may authorize a waiver.
        authority: ContentReference,
        /// Required waiver-authorization evidence declaration.
        evidence: EvidenceRequirementId,
    },
}

impl WaiverPolicy {
    /// Returns whether the contract permits authorized waivers.
    #[must_use]
    pub const fn is_allowed(&self) -> bool { matches!(self, Self::Allowed { .. }) }

    /// Returns the authority policy reference, when waivers are allowed.
    #[must_use]
    pub const fn authority(&self) -> Option<ContentReference> {
        match self {
            Self::Forbidden => None,
            Self::Allowed { authority, .. } => Some(*authority),
        }
    }

    /// Returns the evidence declaration required for every waiver.
    #[must_use]
    pub const fn evidence_requirement(&self) -> Option<EvidenceRequirementId> {
        match self {
            Self::Forbidden => None,
            Self::Allowed { evidence, .. } => Some(*evidence),
        }
    }
}

} // verus!
