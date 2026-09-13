//! Contract-derived review categories, quorum, and supplied independence facts.

#[cfg(verus_only)]
use crate::{AcceptanceEvidence, ReviewObservation, ReviewerIdentity};
#[cfg(verus_only)]
use peritus_spec::{AcceptanceContract, ReviewCategory, ReviewerIndependence};
#[cfg(verus_only)]
use peritus_types::{ActorId, RevisionTuple, Sha256Digest};
use vstd::prelude::*;

verus! {

/// Supplied reviewer provenance dimension compared by the acceptance policy.
#[derive(Clone, Copy)]
pub enum ReviewIndependenceDimension {
    /// Exact review-context digest.
    Context,
    /// Exact model-family digest.
    ModelFamily,
    /// Exact provider digest.
    Provider,
    /// Exact declared ancestry digest.
    Ancestry,
}

/// Equality of all bytes of the two supplied digests.
pub open spec fn digests_match(left: Sha256Digest, right: Sha256Digest) -> bool {
    forall |index: int| 0 <= index < 32 ==>
        left.spec_bytes()[index] == right.spec_bytes()[index]
}

/// Exact identity equality of two content-addressed review categories.
pub open spec fn review_categories_match(left: ReviewCategory, right: ReviewCategory) -> bool {
    digests_match(left.spec_digest(), right.spec_digest())
}

/// The category occurs in the supplied declaration or observation sequence.
pub open spec fn category_present(categories: Seq<ReviewCategory>, target: ReviewCategory) -> bool {
    exists |index: int| 0 <= index < categories.len()
        && review_categories_match(#[trigger] categories[index], target)
}

/// Every observed category is a member of the declared category sequence.
pub open spec fn categories_declared(categories: Seq<ReviewCategory>, declared: Seq<ReviewCategory>) -> bool {
    forall |index: int| 0 <= index < categories.len() ==>
        category_present(declared, #[trigger] categories[index])
}

/// At least one current review reports the required category.
pub open spec fn current_category_covered(
    reviews: Seq<ReviewObservation>,
    target: ReviewCategory,
    requested: RevisionTuple,
) -> bool {
    exists |index: int| 0 <= index < reviews.len()
        && crate::model::revision_fresh(#[trigger] reviews[index].spec_revision(), requested)
        && category_present(reviews[index].spec_categories(), target)
}

/// Number of current observations in a valid prefix, counting each observation once.
pub open spec fn current_review_count_prefix(
    reviews: Seq<ReviewObservation>,
    requested: RevisionTuple,
    end: nat,
) -> nat
    decreases end,
{
    if end == 0 || end > reviews.len() {
        0
    } else {
        current_review_count_prefix(reviews, requested, (end - 1) as nat)
            + if crate::model::revision_fresh(reviews[end as int - 1].spec_revision(), requested) {
                1nat
            } else {
                0nat
            }
    }
}

/// Runtime representation of the review count, saturated at the largest quorum value.
pub open spec fn saturated_review_count(count: nat) -> nat {
    if count > u16::MAX as nat { u16::MAX as nat } else { count }
}

/// Every category on every current observation is contract-declared.
pub open spec fn current_review_categories_declared(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
) -> bool {
    forall |review: int|
        0 <= review < evidence.spec_reviews().len()
        && crate::model::revision_fresh(#[trigger] evidence.spec_reviews()[review].spec_revision(), requested)
        ==> categories_declared(evidence.spec_reviews()[review].spec_categories(),
            contract.spec_review_policy().spec_required_categories())
}

/// Every required category is covered by at least one current review.
pub open spec fn required_review_categories_covered(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
) -> bool {
    forall |index: int| 0 <= index < contract.spec_review_policy().spec_required_categories().len()
        ==> current_category_covered(evidence.spec_reviews(),
            #[trigger] contract.spec_review_policy().spec_required_categories()[index], requested)
}

/// Two distinct positions both contain observations for the requested revision.
pub open spec fn current_review_pair(
    reviews: Seq<ReviewObservation>,
    requested: RevisionTuple,
    left: int,
    right: int,
) -> bool {
    0 <= left < right < reviews.len()
        && crate::model::revision_fresh(reviews[left].spec_revision(), requested)
        && crate::model::revision_fresh(reviews[right].spec_revision(), requested)
}

/// Exact actor-identity byte equality.
pub open spec fn reviewer_actors_match(left: ActorId, right: ActorId) -> bool {
    crate::revision::same_identifier(left.spec_bytes(), right.spec_bytes())
}

/// The supplied provenance digest for the selected independence dimension.
pub open spec fn reviewer_fact(
    identity: ReviewerIdentity,
    dimension: ReviewIndependenceDimension,
) -> Sha256Digest {
    match dimension {
        ReviewIndependenceDimension::Context => identity.spec_context(),
        ReviewIndependenceDimension::ModelFamily => identity.spec_model_family(),
        ReviewIndependenceDimension::Provider => identity.spec_provider(),
        ReviewIndependenceDimension::Ancestry => identity.spec_ancestry(),
    }
}

/// Two current reviews supply the same actor identity.
pub open spec fn duplicate_reviewer_actor(reviews: Seq<ReviewObservation>, requested: RevisionTuple) -> bool {
    exists |left: int, right: int| #[trigger] current_review_pair(reviews, requested, left, right)
        && reviewer_actors_match(reviews[left].spec_reviewer().spec_actor_id(),
            reviews[right].spec_reviewer().spec_actor_id())
}

/// Two current reviews supply the same digest for the selected provenance dimension.
pub open spec fn duplicate_reviewer_fact(
    reviews: Seq<ReviewObservation>,
    requested: RevisionTuple,
    dimension: ReviewIndependenceDimension,
) -> bool {
    exists |left: int, right: int| #[trigger] current_review_pair(reviews, requested, left, right)
        && digests_match(reviewer_fact(reviews[left].spec_reviewer(), dimension),
            reviewer_fact(reviews[right].spec_reviewer(), dimension))
}

/// Every current review carries the producer-independence attestation.
pub open spec fn current_reviews_attest_producer_independence(
    reviews: Seq<ReviewObservation>, requested: RevisionTuple,
) -> bool {
    forall |index: int| 0 <= index < reviews.len()
        && crate::model::revision_fresh(#[trigger] reviews[index].spec_revision(), requested)
        ==> reviews[index].spec_reviewer().spec_independent_from_producer()
}

/// Every configured independence check holds over the supplied current reviewer facts.
///
/// This checks the supplied attestations and identities, not their external authenticity.
pub open spec fn review_independence_complete(
    policy: ReviewerIndependence,
    reviews: Seq<ReviewObservation>,
    requested: RevisionTuple,
) -> bool {
    (policy.spec_distinct_reviewers() ==> !duplicate_reviewer_actor(reviews, requested))
        && (policy.spec_independent_from_producer() ==>
            current_reviews_attest_producer_independence(reviews, requested))
        && (policy.spec_distinct_contexts() ==>
            !duplicate_reviewer_fact(reviews, requested, ReviewIndependenceDimension::Context))
        && (policy.spec_distinct_model_families() ==>
            !duplicate_reviewer_fact(reviews, requested, ReviewIndependenceDimension::ModelFamily))
        && (policy.spec_distinct_providers() ==>
            !duplicate_reviewer_fact(reviews, requested, ReviewIndependenceDimension::Provider))
        && (policy.spec_no_shared_ancestry() ==>
            !duplicate_reviewer_fact(reviews, requested, ReviewIndependenceDimension::Ancestry))
}

/// Exact review-family completeness, independent of evaluator flags and diagnostics.
pub open spec fn required_reviews_complete(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    maximum_cycles: u16,
) -> bool {
    current_review_categories_declared(contract, requested, evidence)
        && required_review_categories_covered(contract, requested, evidence)
        && current_review_count_prefix(evidence.spec_reviews(), requested, evidence.spec_reviews().len())
            >= contract.spec_review_policy().spec_reviewer_quorum()
        && review_independence_complete(contract.spec_review_policy().spec_independence(),
            evidence.spec_reviews(), requested)
        && crate::model::review_cycles_within_limit(evidence.spec_reviews(), requested, maximum_cycles)
}

} // verus!
