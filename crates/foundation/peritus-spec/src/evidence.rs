//! Required evidence and explicit human-authority declarations.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{ContentReference, EvidenceRequirementId, ReviewCategory};
use peritus_types::GateId;

#[doc = " Contract-declared producer class for required evidence."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum EvidenceSource {
    #[doc = " Evidence not owned by a gate, review category, approval, or waiver."]
    General,
    #[doc = " Evidence produced by one declared gate."]
    Gate(GateId),
    #[doc = " Evidence produced by a review of one declared category."]
    Review(ReviewCategory),
    #[doc = " Evidence of explicit human approval."]
    HumanApproval,
    #[doc = " Evidence authorizing a blocker waiver."]
    WaiverAuthorization,
}
#[doc = " Export handling declared for an evidence item."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExportClassification {
    #[doc = " Safe for the ordinary evidence bundle."]
    Public,
    #[doc = " Included only in controlled project-internal exports."]
    Internal,
    #[doc = " Included only in separately authorized restricted exports."]
    Restricted,
}
#[doc = " One required evidence item and its immutable meaning."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvidenceRequirement {
    id: EvidenceRequirementId,
    description: ContentReference,
    source: EvidenceSource,
    export: ExportClassification,
}
impl EvidenceRequirement {
    #[doc = " Creates a required evidence declaration."]
    #[must_use]
    pub const fn new(
        id: EvidenceRequirementId,
        description: ContentReference,
        source: EvidenceSource,
        export: ExportClassification,
    ) -> Self {
        Self { id, description, source, export }
    }
    #[doc = " Returns the stable evidence requirement identifier."]
    #[must_use]
    pub const fn id(&self) -> EvidenceRequirementId {
        self.id
    }
    #[doc = " Returns the immutable description reference."]
    #[must_use]
    pub const fn description(&self) -> ContentReference {
        self.description
    }
    #[doc = " Returns the declared evidence producer class."]
    #[must_use]
    pub const fn source(&self) -> EvidenceSource {
        self.source
    }
    #[doc = " Returns the declared export handling."]
    #[must_use]
    pub const fn export_classification(&self) -> ExportClassification {
        self.export
    }
}
#[doc = " Explicit declaration of whether final human approval is required."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum HumanApprovalPolicy {
    #[doc = " The contract does not require final human approval."]
    NotRequired,
    #[doc = " Acceptance requires approval under the referenced immutable policy."]
    Required(ContentReference),
}
impl HumanApprovalPolicy {
    #[doc = " Returns whether an explicit approval observation is required."]
    #[must_use]
    pub const fn is_required(&self) -> bool {
        matches!(self, Self::Required(_))
    }
    #[doc = " Returns the authority policy reference when approval is required."]
    #[must_use]
    pub const fn authority(&self) -> Option<ContentReference> {
        match self {
            Self::NotRequired => None,
            Self::Required(reference) => Some(*reference),
        }
    }
}
#[doc = " Explicit declaration governing blocker waivers."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WaiverPolicy {
    #[doc = " Blockers cannot be waived under this contract."]
    Forbidden,
    #[doc = " Waivers require the named authority policy and evidence declaration."]
    Allowed {
        #[doc = " Immutable policy describing who may authorize a waiver."]
        authority: ContentReference,
        #[doc = " Required waiver-authorization evidence declaration."]
        evidence: EvidenceRequirementId,
    },
}
impl WaiverPolicy {
    #[doc = " Returns whether the contract permits authorized waivers."]
    #[must_use]
    pub const fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed { .. })
    }
    #[doc = " Returns the authority policy reference, when waivers are allowed."]
    #[must_use]
    pub const fn authority(&self) -> Option<ContentReference> {
        match self {
            Self::Forbidden => None,
            Self::Allowed { authority, .. } => Some(*authority),
        }
    }
    #[doc = " Returns the evidence declaration required for every waiver."]
    #[must_use]
    pub const fn evidence_requirement(&self) -> Option<EvidenceRequirementId> {
        match self {
            Self::Forbidden => None,
            Self::Allowed { evidence, .. } => Some(*evidence),
        }
    }
}
