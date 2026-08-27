//! Independent external review and finding lifecycle observations.

#![allow(missing_docs, reason = "Verus generates ghost enum projection methods")]

use crate::{EvidenceCollection, EvidenceError, EvidenceErrorKind, IntegratedCandidate};
use peritus_types::{ActorId, FindingId, Sha256Digest};
use vstd::prelude::*;

verus! {

/// External reviewer identity and independence facts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReviewerIdentity {
    actor: ActorId,
    organization: Sha256Digest,
    context: Sha256Digest,
}

impl ReviewerIdentity {
    /// Creates exact reviewer identity evidence.
    #[must_use]
    pub const fn new(
        actor: ActorId,
        organization: Sha256Digest,
        context: Sha256Digest,
    ) -> Self {
        Self { actor, organization, context }
    }

    /// Returns the stable reviewer actor.
    #[must_use]
    pub const fn actor(&self) -> ActorId { self.actor }

    /// Returns the external organization identity digest.
    #[must_use]
    pub const fn organization(&self) -> Sha256Digest { self.organization }

    /// Returns the fresh review-context digest.
    #[must_use]
    pub const fn context(&self) -> Sha256Digest { self.context }
}

/// Review completion state supplied by the external review boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReviewCompletion {
    /// The agreed review scope was not completed.
    Incomplete,
    /// The complete scope was reviewed and a final report was issued.
    Completed,
}

/// Mandatory independent external-review scope.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReviewScope {
    /// Native sandbox and escape-focused testing on every tier-one platform.
    SandboxEscape,
    /// Capability, writer, reviewer, fixer, plugin, and MCP authority isolation.
    AuthorityIsolation,
    /// Sealed evaluation, profile protection, promotion, and rollback isolation.
    EvolutionAndPromotion,
    /// Dependency, artifact, SBOM, provenance, license, and signature integrity.
    SupplyChain,
    /// Unsafe-code and trusted-computing-base inventory completeness.
    UnsafeAndTrustedComputingBase,
}

impl ReviewScope {
    /// Complete canonical independent-review scope.
    pub const ALL: [Self; 5] = [
        Self::SandboxEscape,
        Self::AuthorityIsolation,
        Self::EvolutionAndPromotion,
        Self::SupplyChain,
        Self::UnsafeAndTrustedComputingBase,
    ];
}

/// Stable security finding severity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FindingSeverity {
    /// Immediate release-blocking compromise or systemic control failure.
    Critical,
    /// Release-blocking exploitable weakness or required-control failure.
    High,
    /// Material weakness that is tracked but is not an H0 blocker by severity alone.
    Medium,
    /// Limited weakness with bounded impact.
    Low,
    /// Informational hardening observation.
    Informational,
}

impl FindingSeverity {
    /// Reports whether this severity blocks H0 readiness while unresolved.
    #[must_use]
    pub const fn is_release_blocking(self) -> bool {
        matches!(self, Self::Critical | Self::High)
    }
}

/// Complete finding lifecycle state.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FindingLifecycle {
    /// Finding remains open.
    Open,
    /// Risk was accepted but the weakness remains present.
    AcceptedRisk {
        /// Digest of the explicit risk-acceptance authority record.
        authority_digest: Sha256Digest,
    },
    /// Remediation was independently retested against the same exact candidate.
    Resolved {
        /// Digest of remediation evidence.
        remediation_digest: Sha256Digest,
        /// Digest of independent retest evidence.
        retest_digest: Sha256Digest,
    },
}

impl FindingLifecycle {
    /// Reports whether remediation and independent retest completed.
    #[must_use]
    pub const fn is_resolved(self) -> bool { matches!(self, Self::Resolved { .. }) }

    /// Reports whether resolution carries nonempty remediation and retest evidence.
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

/// One finding bound to the reviewed integrated candidate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FindingObservation {
    finding_id: FindingId,
    candidate: IntegratedCandidate,
    severity: FindingSeverity,
    lifecycle: FindingLifecycle,
}

impl FindingObservation {
    /// Creates an exact-candidate finding lifecycle observation.
    #[must_use]
    pub const fn new(
        finding_id: FindingId,
        candidate: IntegratedCandidate,
        severity: FindingSeverity,
        lifecycle: FindingLifecycle,
    ) -> Self {
        Self { finding_id, candidate, severity, lifecycle }
    }

    /// Returns the stable finding identity.
    #[must_use]
    pub const fn finding_id(&self) -> FindingId { self.finding_id }

    /// Returns the exact candidate reviewed for this finding state.
    #[must_use]
    pub const fn candidate(&self) -> (result: IntegratedCandidate)
        ensures result == self.spec_candidate(),
    {
        self.candidate
    }

    /// Returns finding severity.
    #[must_use]
    pub const fn severity(&self) -> FindingSeverity { self.severity }

    /// Returns the complete lifecycle state.
    #[must_use]
    pub const fn lifecycle(&self) -> FindingLifecycle { self.lifecycle }

    /// Specification view of the finding candidate.
    pub closed spec fn spec_candidate(&self) -> IntegratedCandidate { self.candidate }
}

/// Independently produced security review and canonical finding register.
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
    /// Validates a review record and its finding order.
    ///
    /// # Errors
    ///
    /// Rejects duplicate or noncanonical finding IDs and finding states bound to another candidate.
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
        while scope_index < scopes.len()
            invariant (scopes.len() == 0 && scope_index == 1) || 1 <= scope_index <= scopes.len(),
            decreases scopes.len() - scope_index,
        {
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
        while index < findings.len()
            invariant 0 <= index <= findings.len(),
            decreases findings.len() - index,
        {
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

    /// Returns the exact reviewed candidate.
    #[must_use]
    pub const fn candidate(&self) -> (result: IntegratedCandidate)
        ensures result == self.spec_candidate(),
    {
        self.candidate
    }

    /// Returns external reviewer identity evidence.
    #[must_use]
    pub const fn reviewer(&self) -> ReviewerIdentity { self.reviewer }

    /// Returns the candidate-producing actor.
    #[must_use]
    pub const fn producer_actor(&self) -> ActorId { self.producer_actor }

    /// Returns the candidate-producing organization identity.
    #[must_use]
    pub const fn producer_organization(&self) -> Sha256Digest { self.producer_organization }

    /// Returns review completion state.
    #[must_use]
    pub const fn completion(&self) -> ReviewCompletion { self.completion }

    /// Borrows independently reviewed scopes in canonical order.
    #[must_use]
    pub const fn scopes(&self) -> (result: &[ReviewScope])
        ensures result@ == self.spec_scopes(),
    {
        self.scopes.as_slice()
    }

    /// Returns the immutable external report digest.
    #[must_use]
    pub const fn report_digest(&self) -> Sha256Digest { self.report_digest }

    /// Borrows findings in stable finding-ID order.
    #[must_use]
    pub const fn findings(&self) -> (result: &[FindingObservation])
        ensures result@ == self.spec_findings(),
    {
        self.findings.as_slice()
    }

    /// Reports actor and organization independence from the candidate producer.
    #[must_use]
    pub fn independent_from_producer(&self) -> bool {
        self.reviewer.actor != self.producer_actor
            && self.reviewer.organization != self.producer_organization
    }

    /// Specification view of the reviewed candidate.
    pub closed spec fn spec_candidate(&self) -> IntegratedCandidate { self.candidate }
    /// Specification view of reviewed scopes.
    pub closed spec fn spec_scopes(&self) -> Seq<ReviewScope> { self.scopes@ }
    /// Specification view of finding observations.
    pub closed spec fn spec_findings(&self) -> Seq<FindingObservation> { self.findings@ }
}

} // verus!
