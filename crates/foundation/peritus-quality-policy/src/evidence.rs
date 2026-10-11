//! Canonical evidence observations and checked evidence aggregate.

use crate::{
    ApprovalObservation, EvidenceError, GateObservation, ReviewObservation, WaiverObservation,
};
use peritus_spec::EvidenceRequirementId;
use peritus_types::{RevisionTuple, Sha256Digest};

mod authority;
mod ordering;
mod reviews;

#[doc = " One required evidence artifact bound to the complete revision tuple."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvidenceObservation {
    requirement_id: EvidenceRequirementId,
    revision: RevisionTuple,
    artifact_digest: Sha256Digest,
}
impl EvidenceObservation {
    #[doc = " Creates an exact-revision evidence observation."]
    #[must_use]
    pub const fn new(
        requirement_id: EvidenceRequirementId,
        revision: RevisionTuple,
        artifact_digest: Sha256Digest,
    ) -> Self {
        Self { requirement_id, revision, artifact_digest }
    }
    #[doc = " Returns the contract evidence requirement identity."]
    #[must_use]
    pub const fn requirement_id(&self) -> EvidenceRequirementId {
        self.requirement_id
    }
    #[doc = " Returns the exact revision from which the evidence was produced."]
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns the content digest of the evidence artifact."]
    #[must_use]
    pub const fn artifact_digest(&self) -> Sha256Digest {
        self.artifact_digest
    }
}
#[doc = " Checked canonical input to the acceptance evaluator."]
#[derive(Debug, Eq, PartialEq)]
pub struct AcceptanceEvidence {
    gates: Vec<GateObservation>,
    reviews: Vec<ReviewObservation>,
    evidence: Vec<EvidenceObservation>,
    approvals: Vec<ApprovalObservation>,
    waivers: Vec<WaiverObservation>,
}
impl AcceptanceEvidence {
    #[doc = " Validates and stores observations in canonical order."]
    #[doc = ""]
    #[doc = " Gate, evidence, approval-request, review-cycle, and waiver-finding identities must be"]
    #[doc = " strictly ascending. Approval subjects must additionally be unique. Reviewer actor reuse is"]
    #[doc = " retained as evidence and evaluated according to the contract\'s independence policy."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Returns the first duplicate, contradiction, or ordering failure in deterministic"]
    #[doc = " collection order."]
    pub fn new(
        gates: Vec<GateObservation>,
        reviews: Vec<ReviewObservation>,
        evidence: Vec<EvidenceObservation>,
        approvals: Vec<ApprovalObservation>,
        waivers: Vec<WaiverObservation>,
    ) -> Result<Self, EvidenceError> {
        ordering::validate_gates(gates.as_slice())?;
        reviews::validate(reviews.as_slice())?;
        ordering::validate_evidence(evidence.as_slice())?;
        authority::validate_approvals(approvals.as_slice())?;
        authority::validate_waivers(waivers.as_slice(), approvals.as_slice())?;
        Ok(Self { gates, reviews, evidence, approvals, waivers })
    }
    #[doc = " Returns canonical gate observations."]
    #[must_use]
    pub const fn gates(&self) -> &[GateObservation] {
        self.gates.as_slice()
    }
    #[doc = " Returns canonical review observations."]
    #[must_use]
    pub const fn reviews(&self) -> &[ReviewObservation] {
        self.reviews.as_slice()
    }
    #[doc = " Returns canonical required-evidence observations."]
    #[must_use]
    pub const fn evidence(&self) -> &[EvidenceObservation] {
        self.evidence.as_slice()
    }
    #[doc = " Returns canonical human approval observations."]
    #[must_use]
    pub const fn approvals(&self) -> &[ApprovalObservation] {
        self.approvals.as_slice()
    }
    #[doc = " Returns canonical waiver observations."]
    #[must_use]
    pub const fn waivers(&self) -> &[WaiverObservation] {
        self.waivers.as_slice()
    }
}
