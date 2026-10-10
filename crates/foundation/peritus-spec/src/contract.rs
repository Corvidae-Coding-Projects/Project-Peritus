//! Complete immutable acceptance contracts and exact revision binding.

use crate::CompletionPolicy;
use crate::{
    Assumption, ContractDocuments, EvidenceRequirement, Exclusion, GateGraph, HumanApprovalPolicy,
    Requirement, ReviewPolicy, SpecError, WaiverPolicy,
};
use peritus_types::{AcceptanceSpecId, RevisionTuple, Sha256Digest};

mod validation;
#[doc = " Fully checked immutable acceptance contract."]
#[derive(Debug, Eq, PartialEq)]
pub struct AcceptanceContract {
    id: AcceptanceSpecId,
    content_digest: Sha256Digest,
    documents: ContractDocuments,
    requirements: Vec<Requirement>,
    exclusions: Vec<Exclusion>,
    assumptions: Vec<Assumption>,
    gates: GateGraph,
    review_policy: ReviewPolicy,
    evidence_requirements: Vec<EvidenceRequirement>,
    completion_policy: CompletionPolicy,
    approval_policy: HumanApprovalPolicy,
    waiver_policy: WaiverPolicy,
}
impl AcceptanceContract {
    #[doc = " Validates and freezes every acceptance component."]
    #[doc = ""]
    #[doc = " Input collections remain in their supplied strict canonical order. The constructor rejects"]
    #[doc = " ambiguity rather than silently sorting or deduplicating caller data."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the first deterministic validation failure."]
    #[allow(clippy::too_many_arguments, reason = "all immutable contract components stay explicit")]
    pub fn new(
        id: AcceptanceSpecId,
        content_digest: Sha256Digest,
        documents: ContractDocuments,
        requirements: Vec<Requirement>,
        exclusions: Vec<Exclusion>,
        assumptions: Vec<Assumption>,
        gates: GateGraph,
        review_policy: ReviewPolicy,
        evidence_requirements: Vec<EvidenceRequirement>,
        completion_policy: CompletionPolicy,
        approval_policy: HumanApprovalPolicy,
        waiver_policy: WaiverPolicy,
    ) -> Result<Self, SpecError> {
        validation::validate_contract(
            requirements.as_slice(),
            exclusions.as_slice(),
            assumptions.as_slice(),
            &gates,
            &review_policy,
            completion_policy,
            evidence_requirements.as_slice(),
            approval_policy,
            waiver_policy,
        )?;
        Ok(Self {
            id,
            content_digest,
            documents,
            requirements,
            exclusions,
            assumptions,
            gates,
            review_policy,
            evidence_requirements,
            completion_policy,
            approval_policy,
            waiver_policy,
        })
    }
    #[doc = " Returns the immutable acceptance-specification identifier."]
    #[must_use]
    pub const fn id(&self) -> AcceptanceSpecId {
        self.id
    }
    #[doc = " Returns the digest of the complete canonical contract document."]
    #[must_use]
    pub const fn content_digest(&self) -> Sha256Digest {
        self.content_digest
    }
    #[doc = " Returns the immutable objective, scope, policy, and terminal-condition documents."]
    #[must_use]
    pub const fn documents(&self) -> ContractDocuments {
        self.documents
    }
    #[doc = " Returns requirements in canonical identifier order."]
    #[must_use]
    pub const fn requirements(&self) -> &[Requirement] {
        self.requirements.as_slice()
    }
    #[doc = " Returns exclusions in canonical content-reference order."]
    #[must_use]
    pub const fn exclusions(&self) -> &[Exclusion] {
        self.exclusions.as_slice()
    }
    #[doc = " Returns assumptions in canonical content-reference order."]
    #[must_use]
    pub const fn assumptions(&self) -> &[Assumption] {
        self.assumptions.as_slice()
    }
    #[doc = " Returns the complete checked gate graph."]
    #[must_use]
    pub const fn gates(&self) -> &GateGraph {
        &self.gates
    }
    #[doc = " Returns the checked review policy."]
    #[must_use]
    pub const fn review_policy(&self) -> &ReviewPolicy {
        &self.review_policy
    }
    #[doc = " Returns evidence declarations in canonical identifier order."]
    #[must_use]
    pub const fn evidence_requirements(&self) -> &[EvidenceRequirement] {
        self.evidence_requirements.as_slice()
    }
    #[doc = " Returns bounded completion policy."]
    #[must_use]
    pub const fn completion_policy(&self) -> CompletionPolicy {
        self.completion_policy
    }
    #[doc = " Returns the explicit final human-approval declaration."]
    #[must_use]
    pub const fn approval_policy(&self) -> HumanApprovalPolicy {
        self.approval_policy
    }
    #[doc = " Returns the explicit blocker-waiver declaration."]
    #[must_use]
    pub const fn waiver_policy(&self) -> WaiverPolicy {
        self.waiver_policy
    }
    #[doc = " Binds this immutable contract to an exact revision tuple."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`SpecError::RevisionBindingMismatch`] when the tuple names another contract."]
    pub const fn bind(&self, revision: RevisionTuple) -> Result<ContractBinding, SpecError> {
        ContractBinding::new(self, revision)
    }
}
#[doc = " Proof-carrying logical binding between a checked contract and an exact revision tuple."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ContractBinding {
    contract_id: AcceptanceSpecId,
    contract_digest: Sha256Digest,
    revision: RevisionTuple,
}
impl ContractBinding {
    #[doc = " Checks exact equality between contract identity and the tuple\'s specification identity."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns [`SpecError::RevisionBindingMismatch`] on any specification-identity mismatch."]
    pub const fn new(
        contract: &AcceptanceContract,
        revision: RevisionTuple,
    ) -> Result<Self, SpecError> {
        if !crate::identity::acceptance_id_matches(revision.acceptance_spec_id(), contract.id()) {
            return Err(SpecError::RevisionBindingMismatch);
        }
        Ok(Self {
            contract_id: contract.id(),
            contract_digest: contract.content_digest(),
            revision,
        })
    }
    #[doc = " Returns the bound contract identifier."]
    #[must_use]
    pub const fn contract_id(&self) -> AcceptanceSpecId {
        self.contract_id
    }
    #[doc = " Returns the bound immutable contract digest."]
    #[must_use]
    pub const fn contract_digest(&self) -> Sha256Digest {
        self.contract_digest
    }
    #[doc = " Returns the exact revision tuple."]
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns whether the binding still matches a supplied tuple exactly."]
    #[must_use]
    #[allow(
        clippy::missing_const_for_fn,
        reason = "RevisionTuple equality is not const under Verus"
    )]
    pub fn matches_revision(&self, revision: RevisionTuple) -> bool {
        revision == self.revision
    }
}
