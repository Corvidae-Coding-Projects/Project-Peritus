//! Review quorum, category, and independence evaluation.

use crate::{
    AcceptanceEvidence, ReviewerIdentity, ReviewerIndependenceFailure, UnmetCondition,
};
use peritus_spec::{AcceptanceContract, ReviewCategory};
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

fn category_declared(contract: &AcceptanceContract, target: ReviewCategory) -> bool {
    let categories = contract.review_policy().required_categories();
    let mut index = 0;
    while index < categories.len()
        invariant 0 <= index <= categories.len(),
        decreases categories.len() - index,
    {
        if categories[index] == target { return true; }
        index += 1;
    }
    false
}

fn category_covered(
    evidence: &AcceptanceEvidence,
    target: ReviewCategory,
    requested: RevisionTuple,
) -> bool {
    let mut review_index = 0;
    while review_index < evidence.reviews().len()
        invariant 0 <= review_index <= evidence.spec_reviews().len(),
        decreases evidence.spec_reviews().len() - review_index,
    {
        if evidence.reviews()[review_index].revision() == requested {
            let categories = evidence.reviews()[review_index].categories();
            let mut category_index = 0;
            while category_index < categories.len()
                invariant 0 <= category_index <= categories.len(),
                decreases categories.len() - category_index,
            {
                if categories[category_index] == target { return true; }
                category_index += 1;
            }
        }
        review_index += 1;
    }
    false
}

fn current_count(evidence: &AcceptanceEvidence, requested: RevisionTuple) -> u16 {
    let mut count = 0u16;
    let mut index = 0;
    while index < evidence.reviews().len()
        invariant 0 <= index <= evidence.spec_reviews().len(),
        decreases evidence.spec_reviews().len() - index,
    {
        if evidence.reviews()[index].revision() == requested && count < u16::MAX {
            count += 1;
        }
        index += 1;
    }
    count
}

#[derive(Clone, Copy)]
enum IndependenceDimension {
    Context,
    ModelFamily,
    Provider,
    Ancestry,
}

const fn independence_fact(
    identity: &ReviewerIdentity,
    dimension: IndependenceDimension,
) -> peritus_types::Sha256Digest {
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
) -> bool {
    let mut right = 0;
    while right < evidence.reviews().len()
        invariant 0 <= right <= evidence.spec_reviews().len(),
        decreases evidence.spec_reviews().len() - right,
    {
        if evidence.reviews()[right].revision() == requested {
            let mut left = 0;
            while left < right
                invariant 0 <= left <= right < evidence.spec_reviews().len(),
                decreases right - left,
            {
                if evidence.reviews()[left].revision() == requested
                    && independence_fact(evidence.reviews()[left].reviewer(), dimension)
                        == independence_fact(evidence.reviews()[right].reviewer(), dimension)
                {
                    return true;
                }
                left += 1;
            }
        }
        right += 1;
    }
    false
}

fn has_duplicate_actor(evidence: &AcceptanceEvidence, requested: RevisionTuple) -> bool {
    let mut right = 0;
    while right < evidence.reviews().len()
        invariant 0 <= right <= evidence.spec_reviews().len(),
        decreases evidence.spec_reviews().len() - right,
    {
        if evidence.reviews()[right].revision() == requested {
            let mut left = 0;
            while left < right
                invariant 0 <= left <= right < evidence.spec_reviews().len(),
                decreases right - left,
            {
                if evidence.reviews()[left].revision() == requested
                    && evidence.reviews()[left].reviewer().actor_id()
                        == evidence.reviews()[right].reviewer().actor_id()
                {
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
    ensures complete ==> crate::model::review_cycles_within_limit(
        evidence.spec_reviews(),
        requested,
        maximum_cycles,
    ),
{
    let mut complete = true;
    let mut review_index = 0;
    while review_index < evidence.reviews().len()
        invariant 0 <= review_index <= evidence.spec_reviews().len(),
        decreases evidence.spec_reviews().len() - review_index,
    {
        if evidence.reviews()[review_index].revision() == requested {
            if evidence.reviews()[review_index].cycle_ordinal().get() > maximum_cycles {
                complete = false;
                unmet.push(UnmetCondition::ReviewCycleLimitExceeded {
                    cycle_id: evidence.reviews()[review_index].cycle_id(),
                    cycle: evidence.reviews()[review_index].cycle_ordinal().get(),
                    maximum: maximum_cycles,
                });
            }
            let categories = evidence.reviews()[review_index].categories();
            let mut category_index = 0;
            while category_index < categories.len()
                invariant 0 <= category_index <= categories.len(),
                decreases categories.len() - category_index,
            {
                if !category_declared(contract, categories[category_index]) {
                    complete = false;
                    unmet.push(UnmetCondition::UnknownReviewCategory(categories[category_index]));
                }
                category_index += 1;
            }
        }
        review_index += 1;
    }

    let observed = current_count(evidence, requested);
    let required = contract.review_policy().reviewer_quorum();
    if observed < required {
        complete = false;
        unmet.push(UnmetCondition::ReviewerQuorum { required, observed });
    }

    let categories = contract.review_policy().required_categories();
    let mut category_index = 0;
    while category_index < categories.len()
        invariant 0 <= category_index <= categories.len(),
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
            invariant 0 <= index <= evidence.spec_reviews().len(),
            decreases evidence.spec_reviews().len() - index,
        {
            if evidence.reviews()[index].revision() == requested
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
