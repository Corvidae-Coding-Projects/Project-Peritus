//! Independent external review and finding lifecycle observations.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

mod reviewer;

use crate::{EvidenceCollection, EvidenceError, EvidenceErrorKind, IntegratedCandidate};
use peritus_types::{ActorId, FindingId, Sha256Digest};

pub use reviewer::{ReviewCompletion, ReviewScope, ReviewerIdentity};
#[doc = " Stable security finding severity."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FindingSeverity {
    #[doc = " Immediate release-blocking compromise or systemic control failure."]
    Critical,
    #[doc = " Release-blocking exploitable weakness or required-control failure."]
    High,
    #[doc = " Material weakness that is tracked but is not an H0 blocker by severity alone."]
    Medium,
    #[doc = " Limited weakness with bounded impact."]
    Low,
    #[doc = " Informational hardening observation."]
    Informational,
}
impl FindingSeverity {
    #[doc = " Reports whether this severity blocks H0 readiness while unresolved."]
    #[must_use]
    pub const fn is_release_blocking(self) -> bool {
        matches!(self, Self::Critical | Self::High)
    }
}
#[doc = " Complete finding lifecycle state."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FindingLifecycle {
    #[doc = " Finding remains open."]
    Open,
    #[doc = " Risk was accepted but the weakness remains present."]
    AcceptedRisk {
        #[doc = " Digest of the explicit risk-acceptance authority record."]
        authority_digest: Sha256Digest,
    },
    #[doc = " Remediation was independently retested against the same exact candidate."]
    Resolved {
        #[doc = " Digest of remediation evidence."]
        remediation_digest: Sha256Digest,
        #[doc = " Digest of independent retest evidence."]
        retest_digest: Sha256Digest,
    },
}
impl FindingLifecycle {
    #[doc = " Reports whether remediation and independent retest completed."]
    #[must_use]
    pub const fn is_resolved(self) -> bool {
        matches!(self, Self::Resolved { .. })
    }
    #[doc = " Reports whether resolution carries nonempty remediation and retest evidence."]
    #[must_use]
    pub const fn has_resolution_evidence(self) -> bool {
        match self {
            Self::Resolved { remediation_digest, retest_digest } => {
                crate::binding::digest_present(remediation_digest)
                    && crate::binding::digest_present(retest_digest)
            }
            Self::Open | Self::AcceptedRisk { .. } => false,
        }
    }
}
#[doc = " One finding bound to the reviewed integrated candidate."]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FindingObservation {
    finding_id: FindingId,
    candidate: IntegratedCandidate,
    severity: FindingSeverity,
    lifecycle: FindingLifecycle,
}
impl FindingObservation {
    #[doc = " Creates an exact-candidate finding lifecycle observation."]
    #[must_use]
    pub const fn new(
        finding_id: FindingId,
        candidate: IntegratedCandidate,
        severity: FindingSeverity,
        lifecycle: FindingLifecycle,
    ) -> Self {
        Self { finding_id, candidate, severity, lifecycle }
    }
    #[doc = " Returns the stable finding identity."]
    #[must_use]
    pub const fn finding_id(&self) -> FindingId {
        self.finding_id
    }
    #[doc = " Returns the exact candidate reviewed for this finding state."]
    #[must_use]
    pub const fn candidate(&self) -> IntegratedCandidate {
        self.candidate
    }
    #[doc = " Returns finding severity."]
    #[must_use]
    pub const fn severity(&self) -> FindingSeverity {
        self.severity
    }
    #[doc = " Returns the complete lifecycle state."]
    #[must_use]
    pub const fn lifecycle(&self) -> FindingLifecycle {
        self.lifecycle
    }
}
#[doc = " Independently produced security review and canonical finding register."]
#[derive(Debug, Eq, PartialEq)]
pub struct IndependentSecurityReview {
    candidate: IntegratedCandidate,
    reviewer: ReviewerIdentity,
    producer_actor: ActorId,
    producer_organization: Sha256Digest,
    completion: ReviewCompletion,
    scopes: Vec<ReviewScope>,
    report_digest: Sha256Digest,
    findings: Vec<FindingObservation>,
}
impl IndependentSecurityReview {
    #[doc = " Validates a review record and its finding order."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects duplicate or noncanonical finding IDs and finding states bound to another candidate."]
    #[allow(
        clippy::too_many_arguments,
        reason = "candidate, independence, scope, completion, report, and findings remain explicit review facts"
    )]
    pub fn new(
        candidate: IntegratedCandidate,
        reviewer: ReviewerIdentity,
        producer_actor: ActorId,
        producer_organization: Sha256Digest,
        completion: ReviewCompletion,
        scopes: Vec<ReviewScope>,
        report_digest: Sha256Digest,
        findings: Vec<FindingObservation>,
    ) -> Result<Self, EvidenceError> {
        let mut scope_index = 1;
        while scope_index < scopes.len() {
            if scopes[scope_index - 1] == scopes[scope_index] {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::DuplicateObservation,
                    EvidenceCollection::ReviewScopes,
                    scope_index,
                ));
            }
            if scopes[scope_index - 1] > scopes[scope_index] {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::NonCanonicalOrder,
                    EvidenceCollection::ReviewScopes,
                    scope_index,
                ));
            }
            scope_index += 1;
        }
        let mut index = 0;
        while index < findings.len() {
            if !crate::binding::candidate_matches(findings[index].candidate(), candidate) {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::NestedCandidateMismatch,
                    EvidenceCollection::Findings,
                    index,
                ));
            }
            if index > 0 {
                let previous = findings[index - 1].finding_id();
                let current = findings[index].finding_id();
                if previous == current {
                    return Err(EvidenceError::new(
                        EvidenceErrorKind::DuplicateObservation,
                        EvidenceCollection::Findings,
                        index,
                    ));
                }
                if previous > current {
                    return Err(EvidenceError::new(
                        EvidenceErrorKind::NonCanonicalOrder,
                        EvidenceCollection::Findings,
                        index,
                    ));
                }
            }
            index += 1;
        }
        Ok(Self {
            candidate,
            reviewer,
            producer_actor,
            producer_organization,
            completion,
            scopes,
            report_digest,
            findings,
        })
    }
    #[doc = " Returns the exact reviewed candidate."]
    #[must_use]
    pub const fn candidate(&self) -> IntegratedCandidate {
        self.candidate
    }
    #[doc = " Returns external reviewer identity evidence."]
    #[must_use]
    pub const fn reviewer(&self) -> ReviewerIdentity {
        self.reviewer
    }
    #[doc = " Returns the candidate-producing actor."]
    #[must_use]
    pub const fn producer_actor(&self) -> ActorId {
        self.producer_actor
    }
    #[doc = " Returns the candidate-producing organization identity."]
    #[must_use]
    pub const fn producer_organization(&self) -> Sha256Digest {
        self.producer_organization
    }
    #[doc = " Returns review completion state."]
    #[must_use]
    pub const fn completion(&self) -> ReviewCompletion {
        self.completion
    }
    #[doc = " Borrows independently reviewed scopes in canonical order."]
    #[must_use]
    pub const fn scopes(&self) -> &[ReviewScope] {
        self.scopes.as_slice()
    }
    #[doc = " Returns the immutable external report digest."]
    #[must_use]
    pub const fn report_digest(&self) -> Sha256Digest {
        self.report_digest
    }
    #[doc = " Borrows findings in stable finding-ID order."]
    #[must_use]
    pub const fn findings(&self) -> &[FindingObservation] {
        self.findings.as_slice()
    }
    #[doc = " Reports actor and organization independence from the candidate producer."]
    #[must_use]
    pub const fn independent_from_producer(&self) -> bool {
        !crate::binding::actor_ids_equal(self.reviewer.actor(), self.producer_actor())
            && !crate::binding::digests_equal(
                self.reviewer.organization(),
                self.producer_organization(),
            )
    }
}
