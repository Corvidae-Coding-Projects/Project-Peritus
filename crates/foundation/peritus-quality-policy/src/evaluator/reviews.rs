//! Review quorum, category, and independence evaluation.

mod coverage;

use coverage::{category_covered, current_count, observed_categories_declared};

use crate::{
    AcceptanceEvidence, ReviewerIdentity, ReviewerIndependenceFailure, UnmetCondition,
};
use crate::model::ReviewIndependenceDimension as IndependenceDimension;
use peritus_spec::AcceptanceContract;
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

fn cycles_within_limit(
    values: &[crate::ReviewObservation],
    requested: RevisionTuple,
    maximum: u16,
) -> (within_limit: bool)
    ensures within_limit == crate::model::review_cycles_within_limit(
        values@,
        requested,
        maximum,
    ),
{
    let mut index = 0;
    while index < values.len()
        invariant
            0 <= index <= values.len(),
            forall |prior: int| 0 <= prior < index
                && crate::model::revision_fresh(
                    #[trigger] values@[prior].spec_revision(), requested)
                ==> values@[prior].spec_cycle_ordinal() <= maximum,
        decreases values.len() - index,
    {
        if crate::revision::revision_matches(values[index].revision(), requested)
            && values[index].cycle_ordinal().get() > maximum
        {
            assert(!crate::model::review_cycles_within_limit(
                values@,
                requested,
                maximum,
            )) by {
                assert(crate::model::revision_fresh(
                    values@[index as int].spec_revision(),
                    requested,
                ));
            };
            return false;
        }
        index += 1;
    }
    true
}

const fn independence_fact(
    identity: &ReviewerIdentity,
    dimension: IndependenceDimension,
) -> (fact: peritus_types::Sha256Digest)
    ensures fact == crate::model::reviewer_fact(*identity, dimension),
{
    match dimension {
        IndependenceDimension::Context => identity.context(),
        IndependenceDimension::ModelFamily => identity.model_family(),
        IndependenceDimension::Provider => identity.provider(),
        IndependenceDimension::Ancestry => identity.ancestry(),
    }
}

fn has_duplicate_fact(
    evidence: &AcceptanceEvidence,
    requested: RevisionTuple,
    dimension: IndependenceDimension,
) -> (duplicate: bool)
    ensures duplicate == crate::model::duplicate_reviewer_fact(evidence.spec_reviews(), requested, dimension),
{
    let mut right = 0;
    while right < evidence.reviews().len()
        invariant
            0 <= right <= evidence.spec_reviews().len(),
            forall |earlier: int, later: int| later < right
                && #[trigger] crate::model::current_review_pair(evidence.spec_reviews(), requested, earlier, later)
                ==> !crate::model::digests_match(
                    crate::model::reviewer_fact(evidence.spec_reviews()[earlier].spec_reviewer(), dimension),
                    crate::model::reviewer_fact(evidence.spec_reviews()[later].spec_reviewer(), dimension)),
        decreases evidence.spec_reviews().len() - right,
    {
        if crate::revision::revision_matches(evidence.reviews()[right].revision(), requested) {
            let mut left = 0;
            while left < right
                invariant
                    0 <= left <= right < evidence.spec_reviews().len(),
                    crate::model::revision_fresh(evidence.spec_reviews()[right as int].spec_revision(), requested),
                    forall |earlier: int| earlier < left
                        && #[trigger] crate::model::current_review_pair(evidence.spec_reviews(), requested, earlier, right as int)
                        ==> !crate::model::digests_match(
                            crate::model::reviewer_fact(evidence.spec_reviews()[earlier].spec_reviewer(), dimension),
                            crate::model::reviewer_fact(evidence.spec_reviews()[right as int].spec_reviewer(), dimension)),
                decreases right - left,
            {
                if crate::revision::revision_matches(evidence.reviews()[left].revision(), requested)
                    && crate::revision::digest_matches(
                        independence_fact(evidence.reviews()[left].reviewer(), dimension),
                        independence_fact(evidence.reviews()[right].reviewer(), dimension))
                {
                    assert(crate::model::current_review_pair(evidence.spec_reviews(), requested, left as int, right as int));
                    return true;
                }
                left += 1;
            }
        }
        right += 1;
    }
    false
}

fn has_duplicate_actor(evidence: &AcceptanceEvidence, requested: RevisionTuple) -> (duplicate: bool)
    ensures duplicate == crate::model::duplicate_reviewer_actor(evidence.spec_reviews(), requested),
{
    let mut right = 0;
    while right < evidence.reviews().len()
        invariant
            0 <= right <= evidence.spec_reviews().len(),
            forall |earlier: int, later: int| later < right
                && #[trigger] crate::model::current_review_pair(evidence.spec_reviews(), requested, earlier, later)
                ==> !crate::model::reviewer_actors_match(
                    evidence.spec_reviews()[earlier].spec_reviewer().spec_actor_id(),
                    evidence.spec_reviews()[later].spec_reviewer().spec_actor_id()),
        decreases evidence.spec_reviews().len() - right,
    {
        if crate::revision::revision_matches(evidence.reviews()[right].revision(), requested) {
            let mut left = 0;
            while left < right
                invariant
                    0 <= left <= right < evidence.spec_reviews().len(),
                    crate::model::revision_fresh(evidence.spec_reviews()[right as int].spec_revision(), requested),
                    forall |earlier: int| earlier < left
                        && #[trigger] crate::model::current_review_pair(evidence.spec_reviews(), requested, earlier, right as int)
                        ==> !crate::model::reviewer_actors_match(
                            evidence.spec_reviews()[earlier].spec_reviewer().spec_actor_id(),
                            evidence.spec_reviews()[right as int].spec_reviewer().spec_actor_id()),
                decreases right - left,
            {
                if crate::revision::revision_matches(evidence.reviews()[left].revision(), requested)
                    && crate::revision::reviewer_actor_matches(
                        evidence.reviews()[left].reviewer().actor_id(),
                        evidence.reviews()[right].reviewer().actor_id())
                {
                    assert(crate::model::current_review_pair(evidence.spec_reviews(), requested, left as int, right as int));
                    return true;
                }
                left += 1;
            }
        }
        right += 1;
    }
    false
}

#[allow(
    clippy::too_many_lines,
    reason = "one explicit phase keeps deterministic review-condition ordering auditable"
)]
pub(super) fn evaluate(
    contract: &AcceptanceContract,
    requested: RevisionTuple,
    evidence: &AcceptanceEvidence,
    maximum_cycles: u16,
    unmet: &mut Vec<UnmetCondition>,
) -> (complete: bool)
    ensures
        complete == crate::model::required_reviews_complete(contract, requested, evidence, maximum_cycles),
        complete ==> crate::model::review_cycles_within_limit(
            evidence.spec_reviews(), requested, maximum_cycles),
        complete ==> final(unmet)@ == old(unmet)@,
{
    let mut complete = true;
    let mut review_index = 0;
    while review_index < evidence.reviews().len()
        invariant
            0 <= review_index <= evidence.spec_reviews().len(),
            complete == (forall |prior: int| 0 <= prior < review_index
                && crate::model::revision_fresh(#[trigger] evidence.spec_reviews()[prior].spec_revision(), requested)
                ==> (evidence.spec_reviews()[prior].spec_cycle_ordinal() <= maximum_cycles
                    && crate::model::categories_declared(evidence.spec_reviews()[prior].spec_categories(),
                        contract.spec_review_policy().spec_required_categories()))),
            complete ==> unmet@ == old(unmet)@,
        decreases evidence.spec_reviews().len() - review_index,
    {
        if crate::revision::revision_matches(evidence.reviews()[review_index].revision(), requested) {
            if evidence.reviews()[review_index].cycle_ordinal().get() > maximum_cycles {
                complete = false;
                unmet.push(UnmetCondition::ReviewCycleLimitExceeded {
                    cycle_id: evidence.reviews()[review_index].cycle_id(),
                    cycle: evidence.reviews()[review_index].cycle_ordinal().get(),
                    maximum: maximum_cycles,
                });
            }
            let categories_complete = observed_categories_declared(
                contract, evidence.reviews()[review_index].categories(), unmet);
            complete = complete && categories_complete;
        }
        review_index += 1;
    }

    let observed = current_count(evidence, requested);
    let required = contract.review_policy().reviewer_quorum();
    assert((observed >= required) == (crate::model::current_review_count_prefix(
        evidence.spec_reviews(), requested, evidence.spec_reviews().len()) >= required));
    if observed < required {
        complete = false;
        unmet.push(UnmetCondition::ReviewerQuorum { required, observed });
    }

    let categories = contract.review_policy().required_categories();
    let mut category_index = 0;
    while category_index < categories.len()
        invariant
            0 <= category_index <= categories.len(),
            categories@ == contract.spec_review_policy().spec_required_categories(),
            complete == (crate::model::current_review_categories_declared(contract, requested, evidence)
                && crate::model::review_cycles_within_limit(evidence.spec_reviews(), requested, maximum_cycles)
                && crate::model::current_review_count_prefix(
                    evidence.spec_reviews(), requested, evidence.spec_reviews().len())
                    >= contract.spec_review_policy().spec_reviewer_quorum()
                && (forall |prior: int| 0 <= prior < category_index ==>
                    crate::model::current_category_covered(evidence.spec_reviews(),
                        #[trigger] categories@[prior], requested))),
            complete ==> unmet@ == old(unmet)@,
        decreases categories.len() - category_index,
    {
        if !category_covered(evidence, categories[category_index], requested) {
            complete = false;
            unmet.push(UnmetCondition::MissingReviewCategory(categories[category_index]));
        }
        category_index += 1;
    }

    let independence = contract.review_policy().independence();
    if independence.requires_distinct_reviewers()
        && has_duplicate_actor(evidence, requested)
    {
        complete = false;
        unmet.push(UnmetCondition::ReviewerIndependence(
            ReviewerIndependenceFailure::DistinctReviewers,
        ));
    }
    if independence.requires_independence_from_producer() {
        let mut index = 0;
        let mut failed = false;
        while index < evidence.reviews().len()
            invariant
                0 <= index <= evidence.spec_reviews().len(),
                failed == (exists |prior: int| 0 <= prior < index
                    && crate::model::revision_fresh(#[trigger] evidence.spec_reviews()[prior].spec_revision(), requested)
                    && !evidence.spec_reviews()[prior].spec_reviewer().spec_independent_from_producer()),
            decreases evidence.spec_reviews().len() - index,
        {
            if crate::revision::revision_matches(evidence.reviews()[index].revision(), requested)
                && !evidence.reviews()[index].reviewer().independent_from_producer()
            {
                failed = true;
            }
            index += 1;
        }
        if failed {
            complete = false;
            unmet.push(UnmetCondition::ReviewerIndependence(
                ReviewerIndependenceFailure::ProducerIndependence,
            ));
        }
    }
    if independence.requires_distinct_contexts()
        && has_duplicate_fact(evidence, requested, IndependenceDimension::Context)
    {
        complete = false;
        unmet.push(UnmetCondition::ReviewerIndependence(
            ReviewerIndependenceFailure::DistinctContexts,
        ));
    }
    if independence.requires_distinct_model_families()
        && has_duplicate_fact(evidence, requested, IndependenceDimension::ModelFamily)
    {
        complete = false;
        unmet.push(UnmetCondition::ReviewerIndependence(
            ReviewerIndependenceFailure::DistinctModelFamilies,
        ));
    }
    if independence.requires_distinct_providers()
        && has_duplicate_fact(evidence, requested, IndependenceDimension::Provider)
    {
        complete = false;
        unmet.push(UnmetCondition::ReviewerIndependence(
            ReviewerIndependenceFailure::DistinctProviders,
        ));
    }
    if independence.requires_no_shared_ancestry()
        && has_duplicate_fact(evidence, requested, IndependenceDimension::Ancestry)
    {
        complete = false;
        unmet.push(UnmetCondition::ReviewerIndependence(
            ReviewerIndependenceFailure::SharedAncestry,
        ));
    }
    complete
        && cycles_within_limit(
            evidence.reviews(),
            requested,
            maximum_cycles,
        )
}

} // verus!
