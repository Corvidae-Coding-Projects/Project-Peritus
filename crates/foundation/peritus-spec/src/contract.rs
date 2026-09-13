//! Complete immutable acceptance contracts and exact revision binding.

use crate::{
    Assumption, ContractDocuments, EvidenceRequirement, Exclusion, GateGraph, HumanApprovalPolicy,
    Requirement, ReviewPolicy, SpecError, WaiverPolicy,
};
use crate::CompletionPolicy;
use peritus_types::{AcceptanceSpecId, RevisionTuple, Sha256Digest};
use vstd::prelude::*;

verus! {

mod validation;

/// Fully checked immutable acceptance contract.
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
    /// Validates and freezes every acceptance component.
    ///
    /// Input collections remain in their supplied strict canonical order. The constructor rejects
    /// ambiguity rather than silently sorting or deduplicating caller data.
    ///
    /// # Errors
    ///
    /// Returns the first deterministic validation failure.
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

    /// Specification view of the immutable contract identifier.
    pub closed spec fn spec_id(&self) -> AcceptanceSpecId { self.id }

    /// Specification view of the immutable contract digest.
    pub closed spec fn spec_content_digest(&self) -> Sha256Digest { self.content_digest }

    /// Returns the immutable acceptance-specification identifier.
    #[must_use]
    pub const fn id(&self) -> (result: AcceptanceSpecId)
        ensures result == self.spec_id()
    { self.id }

    /// Returns the digest of the complete canonical contract document.
    #[must_use]
    pub const fn content_digest(&self) -> (result: Sha256Digest)
        ensures result == self.spec_content_digest()
    { self.content_digest }

    /// Returns the immutable objective, scope, policy, and terminal-condition documents.
    #[must_use]
    pub const fn documents(&self) -> ContractDocuments { self.documents }

    /// Returns requirements in canonical identifier order.
    #[must_use]
    pub const fn requirements(&self) -> &[Requirement] { self.requirements.as_slice() }

    /// Returns exclusions in canonical content-reference order.
    #[must_use]
    pub const fn exclusions(&self) -> &[Exclusion] { self.exclusions.as_slice() }

    /// Returns assumptions in canonical content-reference order.
    #[must_use]
    pub const fn assumptions(&self) -> &[Assumption] { self.assumptions.as_slice() }

    /// Returns the complete checked gate graph.
    #[must_use]
    pub const fn gates(&self) -> &GateGraph { &self.gates }

    /// Returns the checked review policy.
    #[must_use]
    pub const fn review_policy(&self) -> &ReviewPolicy { &self.review_policy }

    /// Returns evidence declarations in canonical identifier order.
    #[must_use]
    pub const fn evidence_requirements(&self) -> &[EvidenceRequirement] {
        self.evidence_requirements.as_slice()
    }

    /// Returns bounded completion policy.
    #[must_use]
    pub const fn completion_policy(&self) -> CompletionPolicy { self.completion_policy }

    /// Returns the explicit final human-approval declaration.
    #[must_use]
    pub const fn approval_policy(&self) -> HumanApprovalPolicy { self.approval_policy }

    /// Returns the explicit blocker-waiver declaration.
    #[must_use]
    pub const fn waiver_policy(&self) -> WaiverPolicy { self.waiver_policy }

    /// Binds this immutable contract to an exact revision tuple.
    ///
    /// # Errors
    ///
    /// Returns [`SpecError::RevisionBindingMismatch`] when the tuple names another contract.
    pub const fn bind(&self, revision: RevisionTuple) -> Result<ContractBinding, SpecError> {
        ContractBinding::new(self, revision)
    }
}

/// Proof-carrying logical binding between a checked contract and an exact revision tuple.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ContractBinding {
    contract_id: AcceptanceSpecId,
    contract_digest: Sha256Digest,
    revision: RevisionTuple,
}

impl ContractBinding {
    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool {
        crate::acceptance_ids_match(
            self.contract_id,
            self.revision.spec_acceptance_spec_id(),
        )
    }

    /// Checks exact equality between contract identity and the tuple's specification identity.
    ///
    /// # Errors
    ///
    /// Returns [`SpecError::RevisionBindingMismatch`] on any specification-identity mismatch.
    pub const fn new(
        contract: &AcceptanceContract,
        revision: RevisionTuple,
    ) -> (result: Result<Self, SpecError>)
        ensures
            match result {
                Ok(binding) => {
                    binding.spec_contract_id() == contract.spec_id()
                        && binding.spec_contract_digest() == contract.spec_content_digest()
                        && binding.spec_revision() == revision
                        && crate::acceptance_ids_match(
                            revision.spec_acceptance_spec_id(),
                            contract.spec_id(),
                        )
                }
                Err(_) => !crate::acceptance_ids_match(
                    revision.spec_acceptance_spec_id(),
                    contract.spec_id(),
                ),
            }
    {
        if !crate::identity::acceptance_id_matches(
            revision.acceptance_spec_id(),
            contract.id(),
        ) {
            return Err(SpecError::RevisionBindingMismatch);
        }
        Ok(Self {
            contract_id: contract.id(),
            contract_digest: contract.content_digest(),
            revision,
        })
    }

    /// Specification view of the bound contract identifier.
    pub closed spec fn spec_contract_id(&self) -> AcceptanceSpecId { self.contract_id }

    /// Specification view of the bound contract digest.
    pub closed spec fn spec_contract_digest(&self) -> Sha256Digest { self.contract_digest }

    /// Specification view of the exact bound revision tuple.
    pub closed spec fn spec_revision(&self) -> RevisionTuple { self.revision }

    /// Returns the bound contract identifier.
    #[must_use]
    pub const fn contract_id(&self) -> (result: AcceptanceSpecId)
        ensures
            result == self.spec_contract_id(),
            crate::acceptance_ids_match(
                result,
                self.spec_revision().spec_acceptance_spec_id(),
            ),
    {
        proof { use_type_invariant(self); }
        self.contract_id
    }

    /// Returns the bound immutable contract digest.
    #[must_use]
    pub const fn contract_digest(&self) -> (result: Sha256Digest)
        ensures result == self.spec_contract_digest()
    { self.contract_digest }

    /// Returns the exact revision tuple.
    #[must_use]
    pub const fn revision(&self) -> (result: RevisionTuple)
        ensures result == self.spec_revision()
    { self.revision }

    /// Returns whether the binding still matches a supplied tuple exactly.
    #[must_use]
    #[allow(clippy::missing_const_for_fn, reason = "RevisionTuple equality is not const under Verus")]
    pub fn matches_revision(&self, revision: RevisionTuple) -> bool {
        revision == self.revision
    }
}

/// Proves that any constructible binding names its tuple's acceptance specification.
pub proof fn binding_names_tuple_specification(tracked binding: ContractBinding)
    ensures crate::acceptance_ids_match(
        binding.spec_contract_id(),
        binding.spec_revision().spec_acceptance_spec_id(),
    ),
{
    use_type_invariant(&binding);
}

} // verus!
