//! Independent review, finding, and waiver observations.

use crate::{
    ConstructionError, EvidenceBinding, FindingId, PrincipalId, ReleaseCandidate, ReviewId,
};
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {

/// Independent review outcome.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReviewOutcome {
    /// Reviewer approves the exact candidate for production evaluation.
    Approved,
    /// Reviewer requires changes before production evaluation.
    ChangesRequired,
}

/// One signed independent review observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReviewObservation {
    id: ReviewId,
    binding: EvidenceBinding,
    reviewer: PrincipalId,
    producer: PrincipalId,
    context_digest: Sha256Digest,
    review_digest: Sha256Digest,
    outcome: ReviewOutcome,
    independent_from_producer: bool,
}

impl ReviewObservation {
    /// Creates one signed review observation.
    ///
    /// # Errors
    ///
    /// Returns a typed error for a placeholder context or review digest.
    #[allow(clippy::too_many_arguments, reason = "review identity, independence, and outcome stay explicit")]
    pub fn new(
        id: ReviewId,
        binding: EvidenceBinding,
        reviewer: PrincipalId,
        producer: PrincipalId,
        context_digest: Sha256Digest,
        review_digest: Sha256Digest,
        outcome: ReviewOutcome,
        independent_from_producer: bool,
    ) -> Result<Self, ConstructionError> {
        crate::candidate::require_digest(context_digest)?;
        crate::candidate::require_digest(review_digest)?;
        Ok(Self {
            id,
            binding,
            reviewer,
            producer,
            context_digest,
            review_digest,
            outcome,
            independent_from_producer,
        })
    }

    /// Returns the stable review identity.
    #[must_use]
    pub const fn id(&self) -> ReviewId { self.id }

    /// Returns the exact evidence binding.
    #[must_use]
    pub const fn binding(&self) -> EvidenceBinding { self.binding }

    /// Returns the reviewer identity.
    #[must_use]
    pub const fn reviewer(&self) -> PrincipalId { self.reviewer }

    /// Returns the candidate producer identity.
    #[must_use]
    pub const fn producer(&self) -> PrincipalId { self.producer }

    /// Returns the fresh-context digest.
    #[must_use]
    pub const fn context_digest(&self) -> Sha256Digest { self.context_digest }

    /// Returns the signed review digest.
    #[must_use]
    pub const fn review_digest(&self) -> Sha256Digest { self.review_digest }

    /// Returns the explicit review outcome.
    #[must_use]
    pub const fn outcome(&self) -> ReviewOutcome { self.outcome }

    /// Returns the independently attested producer-separation status.
    #[must_use]
    pub const fn independent_from_producer(&self) -> bool { self.independent_from_producer }
}

/// Stable release finding severity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum FindingSeverity {
    /// Informational concern.
    Informational,
    /// Low-impact concern.
    Low,
    /// Material but non-release-blocking concern.
    Medium,
    /// High-impact concern.
    High,
    /// Critical concern.
    Critical,
}

/// Current explicit finding disposition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FindingDisposition {
    /// Finding remains open.
    Open,
    /// Finding was resolved with retained evidence.
    Resolved,
    /// Finding requests an independent waiver.
    WaiverRequested,
    /// Finding was ignored; this always blocks release.
    Ignored,
    /// Finding was quarantined; this always blocks release.
    Quarantined,
}

/// One current finding state bound to the exact candidate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FindingObservation {
    id: FindingId,
    binding: EvidenceBinding,
    reporter: PrincipalId,
    severity: FindingSeverity,
    release_blocking: bool,
    disposition: FindingDisposition,
    finding_digest: Sha256Digest,
}

impl FindingObservation {
    /// Creates one signed finding-state observation.
    ///
    /// # Errors
    ///
    /// Returns a typed error for a placeholder finding digest.
    pub fn new(
        id: FindingId,
        binding: EvidenceBinding,
        reporter: PrincipalId,
        severity: FindingSeverity,
        release_blocking: bool,
        disposition: FindingDisposition,
        finding_digest: Sha256Digest,
    ) -> Result<Self, ConstructionError> {
        crate::candidate::require_digest(finding_digest)?;
        Ok(Self {
            id,
            binding,
            reporter,
            severity,
            release_blocking,
            disposition,
            finding_digest,
        })
    }

    /// Returns the stable finding identity.
    #[must_use]
    pub const fn id(&self) -> FindingId { self.id }

    /// Returns the exact evidence binding.
    #[must_use]
    pub const fn binding(&self) -> EvidenceBinding { self.binding }

    /// Returns the finding reporter.
    #[must_use]
    pub const fn reporter(&self) -> PrincipalId { self.reporter }

    /// Returns the severity.
    #[must_use]
    pub const fn severity(&self) -> FindingSeverity { self.severity }

    /// Returns whether policy marked the finding release-blocking.
    #[must_use]
    pub const fn release_blocking(&self) -> bool { self.release_blocking }

    /// Returns the current disposition.
    #[must_use]
    pub const fn disposition(&self) -> FindingDisposition { self.disposition }

    /// Returns the signed finding-state digest.
    #[must_use]
    pub const fn finding_digest(&self) -> Sha256Digest { self.finding_digest }
}

/// Explicit independent waiver for one exact non-release-blocking finding.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WaiverObservation {
    finding_id: FindingId,
    binding: EvidenceBinding,
    authority: PrincipalId,
    waiver_digest: Sha256Digest,
    justification_digest: Sha256Digest,
    approved: bool,
}

impl WaiverObservation {
    /// Creates one signed waiver observation.
    ///
    /// # Errors
    ///
    /// Returns a typed error for a placeholder waiver or justification digest.
    pub fn new(
        finding_id: FindingId,
        binding: EvidenceBinding,
        authority: PrincipalId,
        waiver_digest: Sha256Digest,
        justification_digest: Sha256Digest,
        approved: bool,
    ) -> Result<Self, ConstructionError> {
        crate::candidate::require_digest(waiver_digest)?;
        crate::candidate::require_digest(justification_digest)?;
        Ok(Self {
            finding_id,
            binding,
            authority,
            waiver_digest,
            justification_digest,
            approved,
        })
    }

    /// Returns the exact finding identity.
    #[must_use]
    pub const fn finding_id(&self) -> FindingId { self.finding_id }

    /// Returns the exact candidate/time/sequence/revision binding.
    #[must_use]
    pub const fn binding(&self) -> EvidenceBinding { self.binding }

    /// Returns the admitted waiver authority.
    #[must_use]
    pub const fn authority(&self) -> PrincipalId { self.authority }

    /// Returns the signed waiver digest.
    #[must_use]
    pub const fn waiver_digest(&self) -> Sha256Digest { self.waiver_digest }

    /// Returns the retained justification digest.
    #[must_use]
    pub const fn justification_digest(&self) -> Sha256Digest { self.justification_digest }

    /// Returns whether authority approved the waiver.
    #[must_use]
    pub const fn approved(&self) -> bool { self.approved }

    /// Returns whether the waiver is exact and current for the evaluation candidate.
    #[must_use]
    pub fn is_current_for(&self, candidate: ReleaseCandidate, evaluated_at: u64) -> bool {
        self.binding.is_current_for(candidate, evaluated_at)
    }
}

} // verus!
