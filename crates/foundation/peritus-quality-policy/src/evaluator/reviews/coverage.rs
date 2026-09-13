//! Exact category coverage and saturated current-review counting.

use crate::{AcceptanceEvidence, UnmetCondition};
use peritus_spec::{AcceptanceContract, ReviewCategory};
use peritus_types::RevisionTuple;
use vstd::prelude::*;

verus! {

fn category_present(categories: &[ReviewCategory], target: ReviewCategory) -> (found: bool)
    ensures found == crate::model::category_present(categories@, target),
{
    let mut index = 0;
    while index < categories.len()
        invariant
            0 <= index <= categories.len(),
            forall |prior: int| 0 <= prior < index ==>
                !crate::model::review_categories_match(#[trigger] categories@[prior], target),
        decreases categories.len() - index,
    {
        if crate::revision::review_category_matches(categories[index], target) { return true; }
        index += 1;
    }
    false
}

pub(super) fn category_covered(
    evidence: &AcceptanceEvidence,
    target: ReviewCategory,
    requested: RevisionTuple,
) -> (covered: bool)
    ensures covered == crate::model::current_category_covered(evidence.spec_reviews(), target, requested),
{
    let mut review_index = 0;
    while review_index < evidence.reviews().len()
        invariant
            0 <= review_index <= evidence.spec_reviews().len(),
            forall |prior: int| 0 <= prior < review_index
                && crate::model::revision_fresh(
                    #[trigger] evidence.spec_reviews()[prior].spec_revision(), requested)
                ==> !crate::model::category_present(
                    evidence.spec_reviews()[prior].spec_categories(), target),
        decreases evidence.spec_reviews().len() - review_index,
    {
        if crate::revision::revision_matches(evidence.reviews()[review_index].revision(), requested)
            && category_present(evidence.reviews()[review_index].categories(), target)
        {
            return true;
        }
        review_index += 1;
    }
    false
}

pub(super) fn current_count(evidence: &AcceptanceEvidence, requested: RevisionTuple) -> (count: u16)
    ensures count as nat == crate::model::saturated_review_count(
        crate::model::current_review_count_prefix(evidence.spec_reviews(), requested, evidence.spec_reviews().len())),
{
    let mut count = 0u16;
    let mut index = 0;
    while index < evidence.reviews().len()
        invariant
            0 <= index <= evidence.spec_reviews().len(),
            count as nat == crate::model::saturated_review_count(
                crate::model::current_review_count_prefix(evidence.spec_reviews(), requested, index as nat)),
        decreases evidence.spec_reviews().len() - index,
    {
        if crate::revision::revision_matches(evidence.reviews()[index].revision(), requested)
            && count < u16::MAX
        {
            count += 1;
        }
        index += 1;
    }
    count
}

pub(super) fn observed_categories_declared(
    contract: &AcceptanceContract,
    categories: &[ReviewCategory],
    unmet: &mut Vec<UnmetCondition>,
) -> (complete: bool)
    ensures complete == crate::model::categories_declared(categories@,
        contract.spec_review_policy().spec_required_categories()),
        complete ==> final(unmet)@ == old(unmet)@,
{
    let mut complete = true;
    let mut category_index = 0;
    while category_index < categories.len()
        invariant
            category_index <= categories.len(),
            complete == (forall |prior: int| 0 <= prior < category_index ==>
                crate::model::category_present(contract.spec_review_policy().spec_required_categories(),
                    #[trigger] categories@[prior])),
            complete ==> unmet@ == old(unmet)@,
        decreases categories.len() - category_index,
    {
        if !category_present(contract.review_policy().required_categories(), categories[category_index]) {
            complete = false;
            unmet.push(UnmetCondition::UnknownReviewCategory(categories[category_index]));
        }
        category_index += 1;
    }
    complete
}

} // verus!
