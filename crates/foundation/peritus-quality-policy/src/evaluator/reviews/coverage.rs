//! Exact category coverage and saturated current-review counting.

use crate::{AcceptanceEvidence, UnmetCondition};
use peritus_spec::{AcceptanceContract, ReviewCategory};
use peritus_types::RevisionTuple;

fn category_present(categories: &[ReviewCategory], target: ReviewCategory) -> bool {
    let mut index = 0;
    while index < categories.len() {
        if crate::revision::review_category_matches(categories[index], target) {
            return true;
        }
        index += 1;
    }
    false
}
pub(super) fn category_covered(
    evidence: &AcceptanceEvidence,
    target: ReviewCategory,
    requested: RevisionTuple,
) -> bool {
    let mut review_index = 0;
    while review_index < evidence.reviews().len() {
        if crate::revision::revision_matches(evidence.reviews()[review_index].revision(), requested)
            && category_present(evidence.reviews()[review_index].categories(), target)
        {
            return true;
        }
        review_index += 1;
    }
    false
}
pub(super) fn current_count(evidence: &AcceptanceEvidence, requested: RevisionTuple) -> u16 {
    let mut count = 0u16;
    let mut index = 0;
    while index < evidence.reviews().len() {
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
) -> bool {
    let mut complete = true;
    let mut category_index = 0;
    while category_index < categories.len() {
        if !category_present(
            contract.review_policy().required_categories(),
            categories[category_index],
        ) {
            complete = false;
            unmet.push(UnmetCondition::UnknownReviewCategory(categories[category_index]));
        }
        category_index += 1;
    }
    complete
}
