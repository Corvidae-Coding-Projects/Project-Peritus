//! Reviewer identity, independence facts, and review observations.

use crate::{
    CanonicalEvidenceCollection, EvidenceError, EvidenceErrorKind, FindingDisposition,
    FindingObservation, ReviewCycleOrdinal,
};
use peritus_spec::ReviewCategory;
use peritus_types::{ActorId, ReviewCycleId, RevisionTuple, Sha256Digest};
use vstd::prelude::*;

verus! {

/// Identity and provenance facts used by reviewer-independence policy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ReviewerIdentity {
    actor_id: ActorId,
    provider: Sha256Digest,
    model_family: Sha256Digest,
    prompt_revision: Sha256Digest,
    context: Sha256Digest,
    ancestry: Sha256Digest,
    independent_from_producer: bool,
}

impl ReviewerIdentity {
    /// Creates reviewer identity and independently attested provenance facts.
    #[must_use]
    pub const fn new(
        actor_id: ActorId,
        provider: Sha256Digest,
        model_family: Sha256Digest,
        prompt_revision: Sha256Digest,
        context: Sha256Digest,
        ancestry: Sha256Digest,
        independent_from_producer: bool,
    ) -> Self {
        Self {
            actor_id,
            provider,
            model_family,
            prompt_revision,
            context,
            ancestry,
            independent_from_producer,
        }
    }

    /// Returns the reviewer actor identity.
    #[must_use]
    pub const fn actor_id(&self) -> ActorId { self.actor_id }

    /// Returns the provider identity digest.
    #[must_use]
    pub const fn provider(&self) -> Sha256Digest { self.provider }

    /// Returns the model-family identity digest.
    #[must_use]
    pub const fn model_family(&self) -> Sha256Digest { self.model_family }

    /// Returns the prompt revision digest.
    #[must_use]
    pub const fn prompt_revision(&self) -> Sha256Digest { self.prompt_revision }

    /// Returns the fresh-context digest.
    #[must_use]
    pub const fn context(&self) -> Sha256Digest { self.context }

    /// Returns the declared shared-ancestry identity digest.
    #[must_use]
    pub const fn ancestry(&self) -> Sha256Digest { self.ancestry }

    /// Returns whether the reviewer is independent from the candidate producer.
    #[must_use]
    pub const fn independent_from_producer(&self) -> bool {
        self.independent_from_producer
    }
}

/// One normalized review bound to a complete revision tuple.
#[derive(Debug, Eq, PartialEq)]
pub struct ReviewObservation {
    cycle_id: ReviewCycleId,
    cycle_ordinal: ReviewCycleOrdinal,
    revision: RevisionTuple,
    reviewer: ReviewerIdentity,
    categories: Vec<ReviewCategory>,
    findings: Vec<FindingObservation>,
    review_digest: Sha256Digest,
}

impl ReviewObservation {
    /// Specification view of the exact reviewed revision.
    pub closed spec fn spec_revision(&self) -> RevisionTuple { self.revision }

    /// Specification view of the one-based review-cycle ordinal.
    pub closed spec fn spec_cycle_ordinal(&self) -> u16 { self.cycle_ordinal.spec_value() }

    /// Specification view of reviewed categories.
    pub closed spec fn spec_categories(&self) -> Seq<ReviewCategory> { self.categories@ }

    /// Specification view of normalized findings.
    pub closed spec fn spec_findings(&self) -> Seq<FindingObservation> { self.findings@ }

    /// Creates a review from canonical, duplicate-free categories and findings.
    ///
    /// # Errors
    ///
    /// Rejects empty or noncanonical categories/findings and resolutions checked on another
    /// revision.
    pub fn new(
        cycle_id: ReviewCycleId,
        cycle_ordinal: ReviewCycleOrdinal,
        revision: RevisionTuple,
        reviewer: ReviewerIdentity,
        categories: Vec<ReviewCategory>,
        findings: Vec<FindingObservation>,
        review_digest: Sha256Digest,
    ) -> Result<Self, EvidenceError> {
        if categories.is_empty() {
            return Err(EvidenceError::new(
                EvidenceErrorKind::EmptyReviewCategories,
                CanonicalEvidenceCollection::ReviewCategories,
                0,
            ));
        }
        let mut index = 1;
        while index < categories.len()
            invariant 1 <= index <= categories.len(),
            decreases categories.len() - index,
        {
            if categories[index - 1] == categories[index] {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::DuplicateObservation,
                    CanonicalEvidenceCollection::ReviewCategories,
                    index,
                ));
            }
            if categories[index - 1] > categories[index] {
                return Err(EvidenceError::new(
                    EvidenceErrorKind::NonCanonicalOrder,
                    CanonicalEvidenceCollection::ReviewCategories,
                    index,
                ));
            }
            index += 1;
        }
        index = 0;
        while index < findings.len()
            invariant 0 <= index <= findings.len(),
            decreases findings.len() - index,
        {
            match findings[index].disposition() {
                FindingDisposition::Resolved { revision: resolution, .. } if resolution != revision => {
                    return Err(EvidenceError::new(
                        EvidenceErrorKind::ResolutionRevisionMismatch,
                        CanonicalEvidenceCollection::Findings,
                        index,
                    ));
                }
                FindingDisposition::Open
                | FindingDisposition::Resolved { .. }
                | FindingDisposition::WaiverRequested => {}
            }
            if index > 0 {
                let previous = findings[index - 1].finding_id();
                let current = findings[index].finding_id();
                if previous == current {
                    return Err(EvidenceError::new(
                        EvidenceErrorKind::DuplicateObservation,
                        CanonicalEvidenceCollection::Findings,
                        index,
                    ));
                }
                if previous > current {
                    return Err(EvidenceError::new(
                        EvidenceErrorKind::NonCanonicalOrder,
                        CanonicalEvidenceCollection::Findings,
                        index,
                    ));
                }
            }
            index += 1;
        }
        Ok(Self {
            cycle_id,
            cycle_ordinal,
            revision,
            reviewer,
            categories,
            findings,
            review_digest,
        })
    }

    /// Returns the review-cycle identity.
    #[must_use]
    pub const fn cycle_id(&self) -> ReviewCycleId { self.cycle_id }

    /// Returns the one-based ordinal of the review cycle.
    #[must_use]
    pub const fn cycle_ordinal(&self) -> (cycle: ReviewCycleOrdinal)
        ensures cycle.spec_value() == self.spec_cycle_ordinal()
    { self.cycle_ordinal }

    /// Returns the exact reviewed revision.
    #[must_use]
    pub const fn revision(&self) -> (revision: RevisionTuple)
        ensures revision == self.spec_revision()
    { self.revision }

    /// Returns reviewer identity and provenance facts.
    #[must_use]
    pub const fn reviewer(&self) -> &ReviewerIdentity { &self.reviewer }

    /// Returns reviewed categories in canonical order.
    #[must_use]
    pub const fn categories(&self) -> (categories: &[ReviewCategory])
        ensures categories@ == self.spec_categories()
    { self.categories.as_slice() }

    /// Returns findings in canonical order.
    #[must_use]
    pub const fn findings(&self) -> (findings: &[FindingObservation])
        ensures findings@ == self.spec_findings()
    { self.findings.as_slice() }

    /// Returns the digest of the normalized review.
    #[must_use]
    pub const fn review_digest(&self) -> Sha256Digest { self.review_digest }
}

} // verus!
