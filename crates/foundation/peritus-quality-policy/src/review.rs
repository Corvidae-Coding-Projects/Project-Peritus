//! Reviewer identity, independence facts, and review observations.

use crate::{EvidenceError, FindingObservation, ReviewCycleOrdinal};

mod validation;
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
    /// Specification view of the supplied reviewer actor.
    pub closed spec fn spec_actor_id(&self) -> ActorId { self.actor_id }

    /// Specification view of the supplied provider identity.
    pub closed spec fn spec_provider(&self) -> Sha256Digest { self.provider }

    /// Specification view of the supplied model-family identity.
    pub closed spec fn spec_model_family(&self) -> Sha256Digest { self.model_family }

    /// Specification view of the supplied prompt revision.
    pub closed spec fn spec_prompt_revision(&self) -> Sha256Digest { self.prompt_revision }

    /// Specification view of the supplied review context.
    pub closed spec fn spec_context(&self) -> Sha256Digest { self.context }

    /// Specification view of the supplied ancestry identity.
    pub closed spec fn spec_ancestry(&self) -> Sha256Digest { self.ancestry }

    /// Specification view of the supplied producer-independence attestation.
    pub closed spec fn spec_independent_from_producer(&self) -> bool { self.independent_from_producer }

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
    ) -> (identity: Self)
        ensures
            identity.spec_actor_id() == actor_id,
            identity.spec_provider() == provider,
            identity.spec_model_family() == model_family,
            identity.spec_prompt_revision() == prompt_revision,
            identity.spec_context() == context,
            identity.spec_ancestry() == ancestry,
            identity.spec_independent_from_producer() == independent_from_producer,
    {
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
    pub const fn actor_id(&self) -> (actor: ActorId)
        ensures actor == self.spec_actor_id(),
    { self.actor_id }

    /// Returns the provider identity digest.
    #[must_use]
    pub const fn provider(&self) -> (provider: Sha256Digest)
        ensures provider == self.spec_provider(),
    { self.provider }

    /// Returns the model-family identity digest.
    #[must_use]
    pub const fn model_family(&self) -> (family: Sha256Digest)
        ensures family == self.spec_model_family(),
    { self.model_family }

    /// Returns the prompt revision digest.
    #[must_use]
    pub const fn prompt_revision(&self) -> (revision: Sha256Digest)
        ensures revision == self.spec_prompt_revision(),
    { self.prompt_revision }

    /// Returns the fresh-context digest.
    #[must_use]
    pub const fn context(&self) -> (context: Sha256Digest)
        ensures context == self.spec_context(),
    { self.context }

    /// Returns the declared shared-ancestry identity digest.
    #[must_use]
    pub const fn ancestry(&self) -> (ancestry: Sha256Digest)
        ensures ancestry == self.spec_ancestry(),
    { self.ancestry }

    /// Returns whether the reviewer is independent from the candidate producer.
    #[must_use]
    pub const fn independent_from_producer(&self) -> (independent: bool)
        ensures independent == self.spec_independent_from_producer(),
    {
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
    #[verifier::type_invariant]
    closed spec fn invariant(&self) -> bool { self.spec_is_canonical() && self.spec_identities_unique() }


    /// Specification view of the review-cycle identity.
    pub closed spec fn spec_cycle_id(&self) -> ReviewCycleId { self.cycle_id }

    /// Specification view of the complete reviewer identity and supplied provenance.
    pub closed spec fn spec_reviewer(&self) -> ReviewerIdentity { self.reviewer }

    /// Specification view of the normalized review digest.
    pub closed spec fn spec_review_digest(&self) -> Sha256Digest { self.review_digest }

    /// Specification view of the exact reviewed revision.
    pub closed spec fn spec_revision(&self) -> RevisionTuple { self.revision }

    /// Specification view of the one-based review-cycle ordinal.
    pub closed spec fn spec_cycle_ordinal(&self) -> u16 { self.cycle_ordinal.spec_value() }

    /// Specification view of reviewed categories.
    pub closed spec fn spec_categories(&self) -> Seq<ReviewCategory> { self.categories@ }

    /// Specification view of normalized findings.
    pub closed spec fn spec_findings(&self) -> Seq<FindingObservation> { self.findings@ }

    /// Specification view of canonical categories, findings, and resolution freshness.
    pub closed spec fn spec_is_canonical(&self) -> bool {
        crate::canonical::reviews::review_admissible(self.categories@, self.findings@, self.revision)
    }

    /// No repeated category or finding identity occurs anywhere in this review.
    pub closed spec fn spec_identities_unique(&self) -> bool {
        crate::canonical::order::unique(crate::canonical::reviews::category_keys(self.categories@))
            && crate::canonical::order::unique(crate::canonical::reviews::finding_keys(self.findings@))
    }

    /// Exposes canonical findings and resolution freshness over the exact observation views.
    pub proof fn canonical_views(&self)
        requires self.spec_is_canonical(),
        ensures crate::canonical::reviews::review_admissible(self.spec_categories(), self.spec_findings(), self.spec_revision()),
    {}

    /// Creates a review from canonical, duplicate-free categories and findings.
    ///
    /// # Errors
    ///
    /// Rejects empty categories, noncanonical categories/findings, and resolutions checked on another
    /// revision.
    pub fn new(
        cycle_id: ReviewCycleId,
        cycle_ordinal: ReviewCycleOrdinal,
        revision: RevisionTuple,
        reviewer: ReviewerIdentity,
        categories: Vec<ReviewCategory>,
        findings: Vec<FindingObservation>,
        review_digest: Sha256Digest,
    ) -> (result: Result<Self, EvidenceError>)
        ensures
            result.is_ok() == crate::canonical::reviews::review_admissible(categories@, findings@, revision),
            match result {
            Ok(review) => review.spec_cycle_id() == cycle_id
                && review.spec_cycle_ordinal() == cycle_ordinal.spec_value()
                && review.spec_revision() == revision
                && review.spec_reviewer() == reviewer
                && review.spec_categories() == categories@
                && review.spec_findings() == findings@
                && review.spec_review_digest() == review_digest
                && review.spec_is_canonical() && review.spec_identities_unique(),
            Err(_) => true,
        },
    {
        validation::categories(categories.as_slice())?;
        validation::findings(findings.as_slice(), revision)?;
        proof { crate::canonical::reviews::canonical_implies_unique(categories@, findings@, revision); }
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
    pub const fn cycle_id(&self) -> (cycle: ReviewCycleId)
        ensures cycle == self.spec_cycle_id(),
    { self.cycle_id }

    /// Returns the one-based ordinal of the review cycle.
    #[must_use]
    pub const fn cycle_ordinal(&self) -> (cycle: ReviewCycleOrdinal)
        ensures cycle.spec_value() == self.spec_cycle_ordinal()
    { self.cycle_ordinal }

    /// Returns the exact reviewed revision.
    #[must_use]
    pub const fn revision(&self) -> (revision: RevisionTuple)
        ensures revision == self.spec_revision(), self.spec_is_canonical(), self.spec_identities_unique(),
    {
        proof { use_type_invariant(self); }
        self.revision
    }

    /// Returns reviewer identity and provenance facts.
    #[must_use]
    pub const fn reviewer(&self) -> (reviewer: &ReviewerIdentity)
        ensures *reviewer == self.spec_reviewer(),
    { &self.reviewer }

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
    pub const fn review_digest(&self) -> (digest: Sha256Digest)
        ensures digest == self.spec_review_digest(),
    { self.review_digest }
}

} // verus!
