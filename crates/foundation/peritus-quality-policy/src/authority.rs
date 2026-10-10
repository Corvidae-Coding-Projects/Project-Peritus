//! Explicit human approval and waiver observations.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use peritus_spec::{ContentReference, EvidenceRequirementId};
use peritus_types::{ActorId, ApprovalRequestId, FindingId, RevisionTuple, Sha256Digest};

#[doc = " Exact purpose for which human authority was requested."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ApprovalSubject {
    #[doc = " Final acceptance of the requested revision."]
    Acceptance,
    #[doc = " Waiver of one exact finding."]
    FindingWaiver(FindingId),
}
#[doc = " Human authority\'s normalized decision."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ApprovalOutcome {
    #[doc = " Authority explicitly approved the subject."]
    Approved,
    #[doc = " Authority explicitly denied the subject."]
    Denied,
}
#[doc = " Authenticated human approval result bound to the complete revision tuple."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ApprovalObservation {
    request_id: ApprovalRequestId,
    revision: RevisionTuple,
    subject: ApprovalSubject,
    actor_id: ActorId,
    authority: ContentReference,
    outcome: ApprovalOutcome,
    evidence_digest: Sha256Digest,
}
impl ApprovalObservation {
    #[doc = " Creates an explicit human-authority observation."]
    #[must_use]
    pub const fn new(
        request_id: ApprovalRequestId,
        revision: RevisionTuple,
        subject: ApprovalSubject,
        actor_id: ActorId,
        authority: ContentReference,
        outcome: ApprovalOutcome,
        evidence_digest: Sha256Digest,
    ) -> Self {
        Self { request_id, revision, subject, actor_id, authority, outcome, evidence_digest }
    }
    #[doc = " Returns the approval request identity."]
    #[must_use]
    pub const fn request_id(&self) -> ApprovalRequestId {
        self.request_id
    }
    #[doc = " Returns the exact revision authorized or denied."]
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns the exact approval purpose."]
    #[must_use]
    pub const fn subject(&self) -> ApprovalSubject {
        self.subject
    }
    #[doc = " Returns the human actor identity."]
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }
    #[doc = " Returns the authority declaration matched against the contract."]
    #[must_use]
    pub const fn authority(&self) -> ContentReference {
        self.authority
    }
    #[doc = " Returns the explicit authority outcome."]
    #[must_use]
    pub const fn outcome(&self) -> ApprovalOutcome {
        self.outcome
    }
    #[doc = " Returns authenticated approval evidence."]
    #[must_use]
    pub const fn evidence_digest(&self) -> Sha256Digest {
        self.evidence_digest
    }
}
#[doc = " Explicit authorization to waive one exact finding on one exact revision."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WaiverObservation {
    finding_id: FindingId,
    revision: RevisionTuple,
    approval_request_id: ApprovalRequestId,
    authority: ContentReference,
    evidence_requirement_id: EvidenceRequirementId,
    waiver_digest: Sha256Digest,
}
impl WaiverObservation {
    #[doc = " Creates an explicit waiver observation."]
    #[must_use]
    pub const fn new(
        finding_id: FindingId,
        revision: RevisionTuple,
        approval_request_id: ApprovalRequestId,
        authority: ContentReference,
        evidence_requirement_id: EvidenceRequirementId,
        waiver_digest: Sha256Digest,
    ) -> Self {
        Self {
            finding_id,
            revision,
            approval_request_id,
            authority,
            evidence_requirement_id,
            waiver_digest,
        }
    }
    #[doc = " Returns the waived finding identity."]
    #[must_use]
    pub const fn finding_id(&self) -> FindingId {
        self.finding_id
    }
    #[doc = " Returns the exact revision on which the waiver applies."]
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns the human approval request authorizing this waiver."]
    #[must_use]
    pub const fn approval_request_id(&self) -> ApprovalRequestId {
        self.approval_request_id
    }
    #[doc = " Returns the contract authority matched by this waiver."]
    #[must_use]
    pub const fn authority(&self) -> ContentReference {
        self.authority
    }
    #[doc = " Returns the contract evidence declaration matched by this waiver."]
    #[must_use]
    pub const fn evidence_requirement_id(&self) -> EvidenceRequirementId {
        self.evidence_requirement_id
    }
    #[doc = " Returns the digest of the authenticated waiver record."]
    #[must_use]
    pub const fn waiver_digest(&self) -> Sha256Digest {
        self.waiver_digest
    }
}
