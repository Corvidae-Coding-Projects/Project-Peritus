//! Review quorum, category, and independence evaluation.

mod coverage;

use coverage::{category_covered, current_count, observed_categories_declared};

use crate::model::ReviewIndependenceDimension as IndependenceDimension;
use crate::{AcceptanceEvidence, ReviewerIdentity, ReviewerIndependenceFailure, UnmetCondition};
use peritus_spec::AcceptanceContract;
use peritus_types::RevisionTuple;

fn cycles_within_limit(
    values: &[crate::ReviewObservation],
    requested: RevisionTuple,
    maximum: u16,
) -> bool {
    let mut index = 0;
    while index < values.len() {
        if crate::revision::revision_matches(values[index].revision(), requested)
            && values[index].cycle_ordinal().get() > maximum
        {
            return false;
        }
        index += 1;
    }
    true
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
    while right < evidence.reviews().len() {
        if crate::revision::revision_matches(evidence.reviews()[right].revision(), requested) {
            let mut left = 0;
            while left < right {
                if crate::revision::revision_matches(evidence.reviews()[left].revision(), requested)
                    && crate::revision::digest_matches(
                        independence_fact(evidence.reviews()[left].reviewer(), dimension),
                        independence_fact(evidence.reviews()[right].reviewer(), dimension),
                    )
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
    while right < evidence.reviews().len() {
        if crate::revision::revision_matches(evidence.reviews()[right].revision(), requested) {
            let mut left = 0;
            while left < right {
                if crate::revision::revision_matches(evidence.reviews()[left].revision(), requested)
                    && crate::revision::reviewer_actor_matches(
                        evidence.reviews()[left].reviewer().actor_id(),
                        evidence.reviews()[right].reviewer().actor_id(),
                    )
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
) -> bool {
    let mut complete = true;
    let mut review_index = 0;
    while review_index < evidence.reviews().len() {
        if crate::revision::revision_matches(evidence.reviews()[review_index].revision(), requested)
        {
            if evidence.reviews()[review_index].cycle_ordinal().get() > maximum_cycles {
                complete = false;
                unmet.push(UnmetCondition::ReviewCycleLimitExceeded {
                    cycle_id: evidence.reviews()[review_index].cycle_id(),
                    cycle: evidence.reviews()[review_index].cycle_ordinal().get(),
                    maximum: maximum_cycles,
                });
            }
            let categories_complete = observed_categories_declared(
                contract,
                evidence.reviews()[review_index].categories(),
                unmet,
            );
            complete = complete && categories_complete;
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
    while category_index < categories.len() {
        if !category_covered(evidence, categories[category_index], requested) {
            complete = false;
            unmet.push(UnmetCondition::MissingReviewCategory(categories[category_index]));
        }
        category_index += 1;
    }
    let independence = contract.review_policy().independence();
    if independence.requires_distinct_reviewers() && has_duplicate_actor(evidence, requested) {
        complete = false;
        unmet.push(UnmetCondition::ReviewerIndependence(
            ReviewerIndependenceFailure::DistinctReviewers,
        ));
    }
    if independence.requires_independence_from_producer() {
        let mut index = 0;
        let mut failed = false;
        while index < evidence.reviews().len() {
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
    complete && cycles_within_limit(evidence.reviews(), requested, maximum_cycles)
}
