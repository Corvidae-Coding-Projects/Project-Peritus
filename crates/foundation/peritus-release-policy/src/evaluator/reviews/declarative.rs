//! Quantified all-observation and quorum characterization of release reviews.

mod pairs;

use crate::{ReleaseCandidate, ReleaseEvidence, ReviewObservation, ReviewOutcome};
use vstd::prelude::*;

verus! {

pub open spec fn review_admitted(
    review: ReviewObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    review.spec_binding().spec_is_current_for(candidate, evaluated_at)
        && review.spec_outcome() == ReviewOutcome::Approved
        && !crate::identity::principal_ids_match(
            review.spec_reviewer(), review.spec_producer(),
        )
        && review.spec_independent_from_producer()
}

pub open spec fn admitted_review_exists_through(
    values: Seq<ReviewObservation>,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: int,
) -> bool {
    exists |index: int| 0 <= index < end
        && #[trigger] review_admitted(values[index], candidate, evaluated_at)
}

pub open spec fn all_reviews_admitted_through(
    values: Seq<ReviewObservation>,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: int,
) -> bool {
    forall |index: int| 0 <= index < end ==>
        #[trigger] review_admitted(values[index], candidate, evaluated_at)
}

pub open spec fn reviewer_quorum_through(
    values: Seq<ReviewObservation>,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: int,
) -> bool {
    exists |pair: (int, int)| 0 <= pair.0 < pair.1 < end
        && #[trigger] review_admitted(values[pair.0], candidate, evaluated_at)
        && #[trigger] review_admitted(values[pair.1], candidate, evaluated_at)
}

pub open spec fn review_pair_clear(
    previous: ReviewObservation,
    current: ReviewObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    !previous.spec_binding().spec_is_current_for(candidate, evaluated_at)
        || !current.spec_binding().spec_is_current_for(candidate, evaluated_at)
        || (!crate::identity::principal_ids_match(
                previous.spec_reviewer(), current.spec_reviewer(),
            )
            && !crate::candidate::digest_matches(
                previous.spec_context_digest(), current.spec_context_digest(),
            )
            && (!crate::identity::review_ids_match(
                    previous.spec_id(), current.spec_id(),
                ) || previous.spec_matches(current)))
}

pub open spec fn review_pairs_clear_through(
    values: Seq<ReviewObservation>,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: int,
) -> bool {
    forall |left: int, right: int| 0 <= left < right < end ==>
        #[trigger] review_pair_clear(
            values[left], values[right], candidate, evaluated_at,
        )
}

/// Direct condition over every supplied review and every review pair.
pub open spec fn review_inputs_ready(
    values: Seq<ReviewObservation>,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    all_reviews_admitted_through(values, candidate, evaluated_at, values.len() as int)
        && reviewer_quorum_through(values, candidate, evaluated_at, values.len() as int)
        && review_pairs_clear_through(values, candidate, evaluated_at, values.len() as int)
}

proof fn characterization_through(
    values: Seq<ReviewObservation>,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
)
    requires end <= values.len(),
    ensures {
        let state = super::model::through(values, candidate, evaluated_at, end);
        &&& (state.approved_count > 0) == admitted_review_exists_through(
            values, candidate, evaluated_at, end as int,
        )
        &&& (state.approved_count >= super::super::MIN_INDEPENDENT_REVIEWERS)
            == reviewer_quorum_through(values, candidate, evaluated_at, end as int)
        &&& (state.stale_count == 0
                && state.mismatched_count == 0
                && state.changes_required_count == 0
                && state.self_review_count == 0
                && state.non_independent_count == 0)
            == all_reviews_admitted_through(
                values, candidate, evaluated_at, end as int,
            )
        &&& (!state.duplicate_reviewer
                && !state.shared_context
                && !state.conflicting_review)
            == review_pairs_clear_through(values, candidate, evaluated_at, end as int)
    },
    decreases end,
{
    if end > 0 {
        let prior_end = (end - 1) as nat;
        characterization_through(values, candidate, evaluated_at, prior_end);
        pairs::pair_reduction_matches_inputs(
            values, candidate, evaluated_at, prior_end,
        );
        let review = values[prior_end as int];
        let admitted = review_admitted(review, candidate, evaluated_at);
        let prior = super::model::through(values, candidate, evaluated_at, prior_end);
        reveal(super::model::through);
        reveal(super::model::step);
        reveal(review_admitted);
        reveal(crate::EvidenceBinding::spec_is_current_for);
        reveal(crate::EvidenceBinding::spec_is_mismatched);
        reveal(crate::EvidenceBinding::spec_is_stale_at);

        assert(admitted_review_exists_through(
            values, candidate, evaluated_at, end as int,
        ) == (admitted_review_exists_through(
            values, candidate, evaluated_at, prior_end as int,
        ) || admitted)) by {
            if admitted_review_exists_through(values, candidate, evaluated_at, end as int)
                && !admitted_review_exists_through(
                    values, candidate, evaluated_at, prior_end as int,
                )
            {
                let witness = choose |index: int| 0 <= index < end
                    && #[trigger] review_admitted(values[index], candidate, evaluated_at);
                assert(witness == prior_end);
            } else if admitted {
                assert(exists |index: int| index == prior_end
                    && 0 <= index < end
                    && #[trigger] review_admitted(values[index], candidate, evaluated_at));
            }
        };

        assert(reviewer_quorum_through(
            values, candidate, evaluated_at, end as int,
        ) == (reviewer_quorum_through(
            values, candidate, evaluated_at, prior_end as int,
        ) || (admitted && admitted_review_exists_through(
            values, candidate, evaluated_at, prior_end as int,
        )))) by {
            if reviewer_quorum_through(values, candidate, evaluated_at, end as int)
                && !reviewer_quorum_through(
                    values, candidate, evaluated_at, prior_end as int,
                )
            {
                reveal(reviewer_quorum_through);
                let pair = choose |pair: (int, int)|
                    #![trigger review_admitted(values[pair.0], candidate, evaluated_at)]
                    #![trigger review_admitted(values[pair.1], candidate, evaluated_at)]
                    0 <= pair.0 < pair.1 < end
                        && review_admitted(values[pair.0], candidate, evaluated_at)
                        && review_admitted(values[pair.1], candidate, evaluated_at);
                let left = pair.0;
                let right = pair.1;
                if right < prior_end {
                    assert(reviewer_quorum_through(
                        values, candidate, evaluated_at, prior_end as int,
                    )) by {
                        assert(exists |earlier: (int, int)|
                            earlier.0 == left
                                && earlier.1 == right
                                && 0 <= earlier.0 < earlier.1 < prior_end
                                && #[trigger] review_admitted(
                                    values[earlier.0], candidate, evaluated_at,
                                )
                                && #[trigger] review_admitted(
                                    values[earlier.1], candidate, evaluated_at,
                                ));
                    }
                }
                assert(right == prior_end);
                assert(admitted_review_exists_through(
                    values, candidate, evaluated_at, prior_end as int,
                ));
            } else if admitted && admitted_review_exists_through(
                values, candidate, evaluated_at, prior_end as int,
            ) {
                reveal(admitted_review_exists_through);
                let witness = choose |index: int| 0 <= index < prior_end
                    && #[trigger] review_admitted(values[index], candidate, evaluated_at);
                assert(0 <= witness < prior_end);
                assert(prior_end < end);
                assert(review_admitted(values[witness], candidate, evaluated_at));
                assert(review_admitted(values[prior_end as int], candidate, evaluated_at));
                assert(exists |pair: (int, int)| 0 <= pair.0 < pair.1 < end
                    && #[trigger] review_admitted(values[pair.0], candidate, evaluated_at)
                    && #[trigger] review_admitted(values[pair.1], candidate, evaluated_at)) by {
                    let pair = (witness, prior_end as int);
                    assert(0 <= pair.0 < pair.1 < end);
                    assert(review_admitted(values[pair.0], candidate, evaluated_at));
                    assert(review_admitted(values[pair.1], candidate, evaluated_at));
                }
            }
        };

        assert(all_reviews_admitted_through(
            values, candidate, evaluated_at, end as int,
        ) == (all_reviews_admitted_through(
            values, candidate, evaluated_at, prior_end as int,
        ) && admitted)) by {
            if all_reviews_admitted_through(values, candidate, evaluated_at, end as int) {
                assert(all_reviews_admitted_through(
                    values, candidate, evaluated_at, prior_end as int,
                ));
            } else if all_reviews_admitted_through(
                values, candidate, evaluated_at, prior_end as int,
            ) && admitted {
                assert forall |index: int| 0 <= index < end implies
                    #[trigger] review_admitted(values[index], candidate, evaluated_at) by {
                    if index < prior_end {
                        assert(review_admitted(values[index], candidate, evaluated_at));
                    } else {
                        assert(index == prior_end);
                    }
                }
            }
        };
        assert(super::super::MIN_INDEPENDENT_REVIEWERS == 2);
        assert((super::super::saturated_increment(prior.approved_count) >= 2)
            == (prior.approved_count > 0));
    }
}

pub(super) proof fn reduction_matches_inputs(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
)
    ensures
        super::model::review_satisfied(
            evidence.spec_reviews(), candidate, evaluated_at,
        ) == review_inputs_ready(evidence.spec_reviews(), candidate, evaluated_at),
{
    characterization_through(
        evidence.spec_reviews(),
        candidate,
        evaluated_at,
        evidence.spec_reviews().len() as nat,
    );
    reveal(super::model::review_satisfied);
    reveal(super::model::state_satisfied);
    reveal(review_inputs_ready);
}

} // verus!
