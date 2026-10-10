//! Reviewer identity, independence facts, and review observations.

use crate::{EvidenceError, FindingObservation, ReviewCycleOrdinal};

mod validation;
use peritus_spec::ReviewCategory;
use peritus_types::{ActorId, ReviewCycleId, RevisionTuple, Sha256Digest};

#[doc = " Identity and provenance facts used by reviewer-independence policy."]
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
    #[doc = " Creates reviewer identity and independently attested provenance facts."]
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
    #[doc = " Returns the reviewer actor identity."]
    #[must_use]
    pub const fn actor_id(&self) -> ActorId {
        self.actor_id
    }
    #[doc = " Returns the provider identity digest."]
    #[must_use]
    pub const fn provider(&self) -> Sha256Digest {
        self.provider
    }
    #[doc = " Returns the model-family identity digest."]
    #[must_use]
    pub const fn model_family(&self) -> Sha256Digest {
        self.model_family
    }
    #[doc = " Returns the prompt revision digest."]
    #[must_use]
    pub const fn prompt_revision(&self) -> Sha256Digest {
        self.prompt_revision
    }
    #[doc = " Returns the fresh-context digest."]
    #[must_use]
    pub const fn context(&self) -> Sha256Digest {
        self.context
    }
    #[doc = " Returns the declared shared-ancestry identity digest."]
    #[must_use]
    pub const fn ancestry(&self) -> Sha256Digest {
        self.ancestry
    }
    #[doc = " Returns whether the reviewer is independent from the candidate producer."]
    #[must_use]
    pub const fn independent_from_producer(&self) -> bool {
        self.independent_from_producer
    }
}
#[doc = " One normalized review bound to a complete revision tuple."]
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
    #[doc = " Creates a review from canonical, duplicate-free categories and findings."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = ""]
    #[doc = " Rejects empty categories, noncanonical categories/findings, and resolutions checked on another"]
    #[doc = " revision."]
    pub fn new(
        cycle_id: ReviewCycleId,
        cycle_ordinal: ReviewCycleOrdinal,
        revision: RevisionTuple,
        reviewer: ReviewerIdentity,
        categories: Vec<ReviewCategory>,
        findings: Vec<FindingObservation>,
        review_digest: Sha256Digest,
    ) -> Result<Self, EvidenceError> {
        validation::categories(categories.as_slice())?;
        validation::findings(findings.as_slice(), revision)?;
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
    #[doc = " Returns the review-cycle identity."]
    #[must_use]
    pub const fn cycle_id(&self) -> ReviewCycleId {
        self.cycle_id
    }
    #[doc = " Returns the one-based ordinal of the review cycle."]
    #[must_use]
    pub const fn cycle_ordinal(&self) -> ReviewCycleOrdinal {
        self.cycle_ordinal
    }
    #[doc = " Returns the exact reviewed revision."]
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    #[doc = " Returns reviewer identity and provenance facts."]
    #[must_use]
    pub const fn reviewer(&self) -> &ReviewerIdentity {
        &self.reviewer
    }
    #[doc = " Returns reviewed categories in canonical order."]
    #[must_use]
    pub const fn categories(&self) -> &[ReviewCategory] {
        self.categories.as_slice()
    }
    #[doc = " Returns findings in canonical order."]
    #[must_use]
    pub const fn findings(&self) -> &[FindingObservation] {
        self.findings.as_slice()
    }
    #[doc = " Returns the digest of the normalized review."]
    #[must_use]
    pub const fn review_digest(&self) -> Sha256Digest {
        self.review_digest
    }
}
