//! Pairwise review reduction correspondence.

use super::{review_pair_clear, review_pairs_clear_through};
use crate::{ReleaseCandidate, ReviewObservation};
use vstd::prelude::*;

verus! {

pub(super) open spec fn current_pairs_clear_through(
    values: Seq<ReviewObservation>,
    current: ReviewObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: int,
) -> bool {
    forall |index: int| 0 <= index < end ==>
        #[trigger] review_pair_clear(values[index], current, candidate, evaluated_at)
}

proof fn current_pair_reduction_matches_inputs(
    values: Seq<ReviewObservation>,
    current: ReviewObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
)
    requires
        end <= values.len(),
        current.spec_binding().spec_is_current_for(candidate, evaluated_at),
    ensures {
        let flags = super::super::model::pairs_through(
            values, current, candidate, evaluated_at, end,
        );
        (!flags.duplicate_reviewer && !flags.shared_context && !flags.conflicting_review)
            == current_pairs_clear_through(
                values, current, candidate, evaluated_at, end as int,
            )
    },
    decreases end,
{
    if end > 0 {
        let prior_end = (end - 1) as nat;
        current_pair_reduction_matches_inputs(
            values, current, candidate, evaluated_at, prior_end,
        );
        reveal(super::super::model::pairs_through);
        reveal(super::super::model::pair_step);
        reveal(review_pair_clear);
        assert(current_pairs_clear_through(
            values, current, candidate, evaluated_at, end as int,
        ) == (current_pairs_clear_through(
            values, current, candidate, evaluated_at, prior_end as int,
        ) && review_pair_clear(
            values[prior_end as int], current, candidate, evaluated_at,
        ))) by {
            if current_pairs_clear_through(
                values, current, candidate, evaluated_at, end as int,
            ) {
                assert(current_pairs_clear_through(
                    values, current, candidate, evaluated_at, prior_end as int,
                ));
            } else if current_pairs_clear_through(
                values, current, candidate, evaluated_at, prior_end as int,
            ) && review_pair_clear(
                values[prior_end as int], current, candidate, evaluated_at,
            ) {
                assert forall |index: int| 0 <= index < end implies
                    #[trigger] review_pair_clear(
                        values[index], current, candidate, evaluated_at,
                    ) by {
                    if index < prior_end {
                        assert(review_pair_clear(
                            values[index], current, candidate, evaluated_at,
                        ));
                    } else {
                        assert(index == prior_end);
                    }
                }
            }
        };
    }
}

pub(super) proof fn pair_reduction_matches_inputs(
    values: Seq<ReviewObservation>,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
)
    requires end < values.len(),
    ensures {
        let prior = super::super::model::through(values, candidate, evaluated_at, end);
        let current = values[end as int];
        let pairs = super::super::model::pairs_through(
            values, current, candidate, evaluated_at, end,
        );
        let next = super::super::model::step(
            prior, values, end, candidate, evaluated_at,
        );
        &&& (!next.duplicate_reviewer && !next.shared_context && !next.conflicting_review)
            == ((!prior.duplicate_reviewer
                    && !prior.shared_context
                    && !prior.conflicting_review)
                && current_pairs_clear_through(
                    values, current, candidate, evaluated_at, end as int,
                ))
        &&& review_pairs_clear_through(
                values, candidate, evaluated_at, (end + 1) as int,
            ) == (review_pairs_clear_through(
                values, candidate, evaluated_at, end as int,
            ) && current_pairs_clear_through(
                values, current, candidate, evaluated_at, end as int,
            ))
    },
{
    let current = values[end as int];
    if current.spec_binding().spec_is_current_for(candidate, evaluated_at) {
        current_pair_reduction_matches_inputs(
            values, current, candidate, evaluated_at, end,
        );
    } else {
        assert(current_pairs_clear_through(
            values, current, candidate, evaluated_at, end as int,
        )) by {
            assert forall |index: int| 0 <= index < end implies
                #[trigger] review_pair_clear(
                    values[index], current, candidate, evaluated_at,
                ) by {
                reveal(review_pair_clear);
            }
        };
    }
    reveal(super::super::model::step);
    assert(review_pairs_clear_through(
        values, candidate, evaluated_at, (end + 1) as int,
    ) == (review_pairs_clear_through(
        values, candidate, evaluated_at, end as int,
    ) && current_pairs_clear_through(
        values, current, candidate, evaluated_at, end as int,
    ))) by {
        if review_pairs_clear_through(values, candidate, evaluated_at, (end + 1) as int) {
            assert(review_pairs_clear_through(values, candidate, evaluated_at, end as int));
            assert(current_pairs_clear_through(
                values, current, candidate, evaluated_at, end as int,
            ));
        } else if review_pairs_clear_through(values, candidate, evaluated_at, end as int)
            && current_pairs_clear_through(
                values, current, candidate, evaluated_at, end as int,
            )
        {
            assert forall |left: int, right: int| 0 <= left < right < end + 1 implies
                #[trigger] review_pair_clear(
                    values[left], values[right], candidate, evaluated_at,
                ) by {
                if right < end {
                    assert(review_pair_clear(
                        values[left], values[right], candidate, evaluated_at,
                    ));
                } else {
                    assert(right == end);
                    assert(review_pair_clear(
                        values[left], current, candidate, evaluated_at,
                    ));
                }
            }
        }
    };
}

} // verus!
