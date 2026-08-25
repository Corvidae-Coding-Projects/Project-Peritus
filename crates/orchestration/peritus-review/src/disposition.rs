//! Append-only finding response and disposition records.

use peritus_evidence::EvidenceId;
use peritus_spec::{ContentReference, EvidenceRequirementId};
use peritus_types::{
    ActorId, ApprovalRequestId, EventId, FindingId, ReviewCycleId, RevisionTuple, Sha256Digest,
};

use crate::ReviewLimits;
use crate::binding::canonical;
use crate::error::{ReviewError, ReviewErrorKind, reject};

/// Closed lifecycle facts retained in finding history.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DispositionKind {
    /// The finding was admitted and remains open.
    Open,
    /// A fixer reported a candidate fix; reviewer confirmation is pending.
    Fixed,
    /// A fixer disputed the finding; reviewer action is pending.
    Disputed,
    /// A fixer proposed another finding as the replacement.
    SupersessionProposed,
    /// An external waiver was requested but not yet authorized.
    WaiverRequested,
    /// An independent reviewer confirmed resolution.
    ResolutionConfirmed,
    /// An independent reviewer confirmed invalidation.
    InvalidationConfirmed,
    /// The finding was superseded with provenance retained by another finding.
    Superseded,
    /// An exact external waiver observation was consumed.
    Waived,
}

/// Structured fixer/requester evidence. None of these variants closes a finding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FixerResponse {
    /// A fixer reports that the finding was addressed.
    Fixed {
        /// Fixer actor identity.
        fixer: ActorId,
        /// Exact revision containing the claimed fix.
        revision: RevisionTuple,
        /// Canonical evidence references.
        evidence: Vec<EvidenceId>,
        /// Digest of the normalized response.
        response_digest: Sha256Digest,
    },
    /// A fixer disputes the finding without closing it.
    Disputed {
        /// Fixer actor identity.
        fixer: ActorId,
        /// Exact disputed revision.
        revision: RevisionTuple,
        /// Canonical evidence references.
        evidence: Vec<EvidenceId>,
        /// Digest of the normalized response.
        response_digest: Sha256Digest,
    },
    /// A fixer proposes provenance-preserving supersession.
    SupersessionProposed {
        /// Fixer actor identity.
        fixer: ActorId,
        /// Exact revision of the proposal.
        revision: RevisionTuple,
        /// Existing proposed canonical replacement.
        superseding: FindingId,
        /// Canonical evidence references.
        evidence: Vec<EvidenceId>,
        /// Digest of the normalized response.
        response_digest: Sha256Digest,
    },
    /// An authority request was created externally; authorization is still absent.
    WaiverRequested {
        /// Actor recording the external request.
        requester: ActorId,
        /// Exact requested revision.
        revision: RevisionTuple,
        /// Existing external approval request identity.
        approval_request_id: ApprovalRequestId,
        /// Contract authority declaration named by the request.
        authority: ContentReference,
        /// Contract evidence declaration named by the request.
        evidence_requirement_id: EvidenceRequirementId,
        /// Digest of the inert external request.
        request_digest: Sha256Digest,
    },
}

impl FixerResponse {
    /// Creates a checked fixed response.
    ///
    /// # Errors
    /// Rejects empty, duplicate, noncanonical, or oversized evidence references.
    pub fn fixed(
        fixer: ActorId,
        revision: RevisionTuple,
        evidence: Vec<EvidenceId>,
        response_digest: Sha256Digest,
        limits: ReviewLimits,
    ) -> Result<Self, ReviewError> {
        validate_evidence(&evidence, limits)?;
        Ok(Self::Fixed { fixer, revision, evidence, response_digest })
    }

    /// Creates a checked dispute response.
    ///
    /// # Errors
    /// Rejects empty, duplicate, noncanonical, or oversized evidence references.
    pub fn disputed(
        fixer: ActorId,
        revision: RevisionTuple,
        evidence: Vec<EvidenceId>,
        response_digest: Sha256Digest,
        limits: ReviewLimits,
    ) -> Result<Self, ReviewError> {
        validate_evidence(&evidence, limits)?;
        Ok(Self::Disputed { fixer, revision, evidence, response_digest })
    }

    /// Creates a checked supersession proposal.
    ///
    /// # Errors
    /// Rejects empty, duplicate, noncanonical, or oversized evidence references.
    pub fn supersession_proposed(
        fixer: ActorId,
        revision: RevisionTuple,
        superseding: FindingId,
        evidence: Vec<EvidenceId>,
        response_digest: Sha256Digest,
        limits: ReviewLimits,
    ) -> Result<Self, ReviewError> {
        validate_evidence(&evidence, limits)?;
        Ok(Self::SupersessionProposed { fixer, revision, superseding, evidence, response_digest })
    }

    /// Records an already-existing external waiver request without authorizing it.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub const fn waiver_requested(
        requester: ActorId,
        revision: RevisionTuple,
        approval_request_id: ApprovalRequestId,
        authority: ContentReference,
        evidence_requirement_id: EvidenceRequirementId,
        request_digest: Sha256Digest,
    ) -> Self {
        Self::WaiverRequested {
            requester,
            revision,
            approval_request_id,
            authority,
            evidence_requirement_id,
            request_digest,
        }
    }

    /// Returns the actor recording the response/request.
    #[must_use]
    pub const fn actor(&self) -> ActorId {
        match self {
            Self::Fixed { fixer, .. }
            | Self::Disputed { fixer, .. }
            | Self::SupersessionProposed { fixer, .. } => *fixer,
            Self::WaiverRequested { requester, .. } => *requester,
        }
    }

    /// Returns the exact affected revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        match self {
            Self::Fixed { revision, .. }
            | Self::Disputed { revision, .. }
            | Self::SupersessionProposed { revision, .. }
            | Self::WaiverRequested { revision, .. } => *revision,
        }
    }

    /// Returns canonical evidence references, empty for an authority request.
    #[must_use]
    pub fn evidence(&self) -> &[EvidenceId] {
        match self {
            Self::Fixed { evidence, .. }
            | Self::Disputed { evidence, .. }
            | Self::SupersessionProposed { evidence, .. } => evidence,
            Self::WaiverRequested { .. } => &[],
        }
    }

    /// Returns the normalized response/request digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        match self {
            Self::Fixed { response_digest, .. }
            | Self::Disputed { response_digest, .. }
            | Self::SupersessionProposed { response_digest, .. } => *response_digest,
            Self::WaiverRequested { request_digest, .. } => *request_digest,
        }
    }

    /// Returns a proposed superseding finding, when present.
    #[must_use]
    pub const fn superseding(&self) -> Option<FindingId> {
        match self {
            Self::SupersessionProposed { superseding, .. } => Some(*superseding),
            _ => None,
        }
    }

    /// Returns waiver request identity, when this is a waiver request.
    #[must_use]
    pub const fn approval_request_id(&self) -> Option<ApprovalRequestId> {
        match self {
            Self::WaiverRequested { approval_request_id, .. } => Some(*approval_request_id),
            _ => None,
        }
    }

    /// Returns requested waiver authority, when present.
    #[must_use]
    pub const fn authority(&self) -> Option<ContentReference> {
        match self {
            Self::WaiverRequested { authority, .. } => Some(*authority),
            _ => None,
        }
    }

    /// Returns requested waiver evidence declaration, when present.
    #[must_use]
    pub const fn evidence_requirement_id(&self) -> Option<EvidenceRequirementId> {
        match self {
            Self::WaiverRequested { evidence_requirement_id, .. } => Some(*evidence_requirement_id),
            _ => None,
        }
    }

    pub(super) fn validate(&self, limits: ReviewLimits) -> Result<(), ReviewError> {
        match self {
            Self::Fixed { evidence, .. }
            | Self::Disputed { evidence, .. }
            | Self::SupersessionProposed { evidence, .. } => validate_evidence(evidence, limits),
            Self::WaiverRequested { .. } => Ok(()),
        }
    }
}

/// One immutable event-stamped append-only disposition fact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DispositionRecord {
    event_id: EventId,
    kind: DispositionKind,
    actor: Option<ActorId>,
    reviewer_cycle: Option<ReviewCycleId>,
    revision: RevisionTuple,
    evidence: Vec<EvidenceId>,
    related_finding: Option<FindingId>,
    approval_request_id: Option<ApprovalRequestId>,
    authority: Option<ContentReference>,
    evidence_requirement_id: Option<EvidenceRequirementId>,
    record_digest: Sha256Digest,
}

impl DispositionRecord {
    #[allow(clippy::too_many_arguments)]
    pub(super) const fn from_wire(
        event_id: EventId,
        kind: DispositionKind,
        actor: Option<ActorId>,
        reviewer_cycle: Option<ReviewCycleId>,
        revision: RevisionTuple,
        evidence: Vec<EvidenceId>,
        related_finding: Option<FindingId>,
        approval_request_id: Option<ApprovalRequestId>,
        authority: Option<ContentReference>,
        evidence_requirement_id: Option<EvidenceRequirementId>,
        record_digest: Sha256Digest,
    ) -> Self {
        Self {
            event_id,
            kind,
            actor,
            reviewer_cycle,
            revision,
            evidence,
            related_finding,
            approval_request_id,
            authority,
            evidence_requirement_id,
            record_digest,
        }
    }

    /// Returns the event carrying this fact.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }
    /// Returns the closed disposition kind.
    #[must_use]
    pub const fn kind(&self) -> DispositionKind {
        self.kind
    }
    /// Returns the actor, when the fact is actor-attributed.
    #[must_use]
    pub const fn actor(&self) -> Option<ActorId> {
        self.actor
    }
    /// Returns the independent confirming cycle, when present.
    #[must_use]
    pub const fn reviewer_cycle(&self) -> Option<ReviewCycleId> {
        self.reviewer_cycle
    }
    /// Returns the exact affected revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Borrows canonical evidence references.
    #[must_use]
    pub const fn evidence(&self) -> &[EvidenceId] {
        self.evidence.as_slice()
    }
    /// Returns a related finding identity, when present.
    #[must_use]
    pub const fn related_finding(&self) -> Option<FindingId> {
        self.related_finding
    }
    /// Returns an external approval request identity, when present.
    #[must_use]
    pub const fn approval_request_id(&self) -> Option<ApprovalRequestId> {
        self.approval_request_id
    }
    /// Returns a contract authority reference, when present.
    #[must_use]
    pub const fn authority(&self) -> Option<ContentReference> {
        self.authority
    }
    /// Returns a contract evidence declaration, when present.
    #[must_use]
    pub const fn evidence_requirement_id(&self) -> Option<EvidenceRequirementId> {
        self.evidence_requirement_id
    }
    /// Returns the response/request/observation digest attached to the fact.
    #[must_use]
    pub const fn record_digest(&self) -> Sha256Digest {
        self.record_digest
    }

    pub(super) fn validate(&self, limits: ReviewLimits) -> Result<(), ReviewError> {
        match self.kind {
            DispositionKind::Open
            | DispositionKind::WaiverRequested
            | DispositionKind::Superseded
            | DispositionKind::Waived => {
                if self.evidence.len() > usize::from(limits.evidence_references()) {
                    Err(reject(
                        ReviewErrorKind::LimitExceeded,
                        "disposition evidence exceeds its limit",
                    ))
                } else {
                    canonical(&self.evidence, "disposition evidence is not canonical")
                }
            }
            DispositionKind::Fixed
            | DispositionKind::Disputed
            | DispositionKind::SupersessionProposed
            | DispositionKind::ResolutionConfirmed
            | DispositionKind::InvalidationConfirmed => validate_evidence(&self.evidence, limits),
        }
    }
}

pub fn validate_evidence(evidence: &[EvidenceId], limits: ReviewLimits) -> Result<(), ReviewError> {
    if evidence.is_empty() || evidence.len() > usize::from(limits.evidence_references()) {
        return Err(reject(
            ReviewErrorKind::EvidenceInvalid,
            "evidence references are empty or exceed their limit",
        ));
    }
    canonical(evidence, "evidence references are not canonical")
}
