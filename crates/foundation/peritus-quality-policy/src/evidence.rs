//! Canonical evidence observations and checked evidence aggregate.

use crate::{
    ApprovalObservation, EvidenceError, GateObservation, ReviewObservation, WaiverObservation,
};
use peritus_spec::EvidenceRequirementId;
use peritus_types::{RevisionTuple, Sha256Digest};
use vstd::prelude::*;

mod ordering;
mod reviews;
mod authority;

verus! {

/// One required evidence artifact bound to the complete revision tuple.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvidenceObservation {
    requirement_id: EvidenceRequirementId,
    revision: RevisionTuple,
    artifact_digest: Sha256Digest,
}

impl EvidenceObservation {
    /// Specification view of the exact required artifact identity.
    pub closed spec fn spec_requirement_id(&self) -> EvidenceRequirementId { self.requirement_id }

    /// Specification view of the exact producing revision.
    pub closed spec fn spec_revision(&self) -> RevisionTuple { self.revision }

    /// Creates an exact-revision evidence observation.
    #[must_use]
    pub const fn new(
        requirement_id: EvidenceRequirementId,
        revision: RevisionTuple,
        artifact_digest: Sha256Digest,
    ) -> Self {
        Self { requirement_id, revision, artifact_digest }
    }

    /// Returns the contract evidence requirement identity.
    #[must_use]
    pub const fn requirement_id(&self) -> (id: EvidenceRequirementId)
        ensures id == self.spec_requirement_id(),
    { self.requirement_id }

    /// Returns the exact revision from which the evidence was produced.
    #[must_use]
    pub const fn revision(&self) -> (revision: RevisionTuple)
        ensures revision == self.spec_revision()
    { self.revision }

    /// Returns the content digest of the evidence artifact.
    #[must_use]
    pub const fn artifact_digest(&self) -> Sha256Digest { self.artifact_digest }
}

/// Checked canonical input to the acceptance evaluator.
#[derive(Debug, Eq, PartialEq)]
pub struct AcceptanceEvidence {
    gates: Vec<GateObservation>,
    reviews: Vec<ReviewObservation>,
    evidence: Vec<EvidenceObservation>,
    approvals: Vec<ApprovalObservation>,
    waivers: Vec<WaiverObservation>,
}

impl AcceptanceEvidence {
    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool { self.spec_is_canonical() && self.spec_identities_unique() }

    /// Primary identities are unique over each complete supplied collection.
    pub closed spec fn spec_identities_unique(&self) -> bool {
        crate::canonical::collections::identities_unique(self.gates@, self.reviews@, self.evidence@, self.approvals@, self.waivers@)
    }

    /// Canonical admission facts for every stored collection.
    pub closed spec fn spec_is_canonical(&self) -> bool {
        crate::canonical::collections::evidence_admissible(self.gates@, self.reviews@, self.evidence@, self.approvals@, self.waivers@)
    }

    /// Specification view of gate observations.
    pub closed spec fn spec_gates(&self) -> Seq<GateObservation> { self.gates@ }

    /// Specification view of review observations.
    pub closed spec fn spec_reviews(&self) -> Seq<ReviewObservation> { self.reviews@ }

    /// Specification view of required-evidence observations.
    pub closed spec fn spec_evidence(&self) -> Seq<EvidenceObservation> { self.evidence@ }

    /// Specification view of approval observations.
    pub closed spec fn spec_approvals(&self) -> Seq<ApprovalObservation> { self.approvals@ }

    /// Specification view of waiver observations.
    pub closed spec fn spec_waivers(&self) -> Seq<WaiverObservation> { self.waivers@ }

    /// Exposes the independently defined collection predicates from the constructor invariant.
    pub proof fn canonical_views(&self)
        requires self.spec_is_canonical(), self.spec_identities_unique(),
        ensures
            crate::canonical::collections::evidence_admissible(self.spec_gates(), self.spec_reviews(), self.spec_evidence(), self.spec_approvals(), self.spec_waivers()),
            crate::canonical::collections::identities_unique(self.spec_gates(), self.spec_reviews(), self.spec_evidence(), self.spec_approvals(), self.spec_waivers()),
    {}

    /// INV-003 predicate over every supplied observation collection.
    pub open spec fn spec_all_current(&self, requested: RevisionTuple) -> bool {
        (forall |index: int| 0 <= index < self.spec_gates().len() ==>
            #[trigger] crate::model::revision_fresh(
                self.spec_gates()[index].spec_revision(), requested))
        && (forall |index: int| 0 <= index < self.spec_reviews().len() ==>
            #[trigger] crate::model::revision_fresh(
                self.spec_reviews()[index].spec_revision(), requested))
        && (forall |index: int| 0 <= index < self.spec_evidence().len() ==>
            #[trigger] crate::model::revision_fresh(
                self.spec_evidence()[index].spec_revision(), requested))
        && (forall |index: int| 0 <= index < self.spec_approvals().len() ==>
            #[trigger] crate::model::revision_fresh(
                self.spec_approvals()[index].spec_revision(), requested))
        && (forall |index: int| 0 <= index < self.spec_waivers().len() ==>
            #[trigger] crate::model::revision_fresh(
                self.spec_waivers()[index].spec_revision(), requested))
    }
    /// Validates and stores observations in canonical order.
    ///
    /// Gate, evidence, approval-request, review-cycle, and waiver-finding identities must be
    /// strictly ascending. Approval subjects must additionally be unique. Reviewer actor reuse is
    /// retained as evidence and evaluated according to the contract's independence policy.
    ///
    /// # Errors
    ///
    /// Returns the first duplicate, contradiction, or ordering failure in deterministic
    /// collection order.
    pub fn new(
        gates: Vec<GateObservation>,
        reviews: Vec<ReviewObservation>,
        evidence: Vec<EvidenceObservation>,
        approvals: Vec<ApprovalObservation>,
        waivers: Vec<WaiverObservation>,
    ) -> (result: Result<Self, EvidenceError>)
        ensures
            result.is_ok() == crate::canonical::collections::evidence_admissible(gates@, reviews@, evidence@, approvals@, waivers@),
            match result {
                Ok(value) => value.spec_gates() == gates@ && value.spec_reviews() == reviews@
                    && value.spec_evidence() == evidence@ && value.spec_approvals() == approvals@
                    && value.spec_waivers() == waivers@ && value.spec_is_canonical() && value.spec_identities_unique(),
                Err(_) => true,
            },
    {
        ordering::validate_gates(gates.as_slice())?;
        reviews::validate(reviews.as_slice())?;
        ordering::validate_evidence(evidence.as_slice())?;
        authority::validate_approvals(approvals.as_slice())?;
        authority::validate_waivers(waivers.as_slice(), approvals.as_slice())?;
        proof { crate::canonical::collections::admissible_implies_unique(gates@, reviews@, evidence@, approvals@, waivers@); }
        Ok(Self { gates, reviews, evidence, approvals, waivers })
    }

    /// Returns canonical gate observations.
    #[must_use]
    pub const fn gates(&self) -> (gates: &[GateObservation])
        ensures gates@ == self.spec_gates(), self.spec_is_canonical(), self.spec_identities_unique(),
    {
        proof { use_type_invariant(self); }
        self.gates.as_slice()
    }

    /// Returns canonical review observations.
    #[must_use]
    pub const fn reviews(&self) -> (reviews: &[ReviewObservation])
        ensures reviews@ == self.spec_reviews(), self.spec_is_canonical(), self.spec_identities_unique(),
    {
        proof { use_type_invariant(self); }
        self.reviews.as_slice()
    }

    /// Returns canonical required-evidence observations.
    #[must_use]
    pub const fn evidence(&self) -> (evidence: &[EvidenceObservation])
        ensures evidence@ == self.spec_evidence(), self.spec_is_canonical(), self.spec_identities_unique(),
    {
        proof { use_type_invariant(self); }
        self.evidence.as_slice()
    }

    /// Returns canonical human approval observations.
    #[must_use]
    pub const fn approvals(&self) -> (approvals: &[ApprovalObservation])
        ensures approvals@ == self.spec_approvals(), self.spec_is_canonical(), self.spec_identities_unique(),
    {
        proof { use_type_invariant(self); }
        self.approvals.as_slice()
    }

    /// Returns canonical waiver observations.
    #[must_use]
    pub const fn waivers(&self) -> (waivers: &[WaiverObservation])
        ensures waivers@ == self.spec_waivers(), self.spec_is_canonical(), self.spec_identities_unique(),
    {
        proof { use_type_invariant(self); }
        self.waivers.as_slice()
    }
}

} // verus!
