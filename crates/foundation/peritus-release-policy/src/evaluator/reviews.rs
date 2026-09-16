//! Independent-review reduction over the supplied observations.

#[cfg(verus_only)]
pub mod model;
#[cfg(verus_only)]
mod declarative;

use crate::{
    ReleaseCandidate, ReleaseEvidence, ReviewAssessment, ReviewObservation, ReviewOutcome,
};
use vstd::prelude::*;

verus! {

/// Exact stored fields produced by the complete supplied-review reduction.
pub open spec fn assessment_matches_reduction(
    assessment: ReviewAssessment,
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    let state = model::through(
        evidence.spec_reviews(),
        candidate,
        evaluated_at,
        evidence.spec_reviews().len() as nat,
    );
    &&& assessment.spec_is_satisfied() == model::state_satisfied(state)
    &&& assessment.spec_approved_count() == state.approved_count
    &&& assessment.spec_stale_count() == state.stale_count
    &&& assessment.spec_mismatched_count() == state.mismatched_count
    &&& assessment.spec_changes_required_count() == state.changes_required_count
    &&& assessment.spec_self_review_count() == state.self_review_count
    &&& assessment.spec_non_independent_count() == state.non_independent_count
    &&& assessment.spec_duplicate_reviewer() == state.duplicate_reviewer
    &&& assessment.spec_shared_context() == state.shared_context
    &&& assessment.spec_conflicting_review() == state.conflicting_review
}

pub open spec fn review_satisfied(
    evidence: &ReleaseEvidence,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    declarative::review_inputs_ready(
        evidence.spec_reviews(), candidate, evaluated_at,
    )
}

fn assess_pairs(
    review: &ReviewObservation,
    right: usize,
    candidate: &ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
    initial: (bool, bool, bool),
) -> (flags: (bool, bool, bool))
    requires right < evidence.spec_reviews().len(),
    ensures
        flags.0 == (initial.0 || model::pairs_through(
            evidence.spec_reviews(),
            *review,
            *candidate,
            evaluated_at,
            right as nat,
        ).duplicate_reviewer),
        flags.1 == (initial.1 || model::pairs_through(
            evidence.spec_reviews(),
            *review,
            *candidate,
            evaluated_at,
            right as nat,
        ).shared_context),
        flags.2 == (initial.2 || model::pairs_through(
            evidence.spec_reviews(),
            *review,
            *candidate,
            evaluated_at,
            right as nat,
        ).conflicting_review),
{
    let mut duplicate_reviewer = initial.0;
    let mut shared_context = initial.1;
    let mut conflicting_review = initial.2;
    let mut left = 0;
    while left < right
        invariant
            0 <= left <= right,
            right < evidence.spec_reviews().len(),
            duplicate_reviewer == (initial.0 || model::pairs_through(
                evidence.spec_reviews(),
                *review,
                *candidate,
                evaluated_at,
                left as nat,
            ).duplicate_reviewer),
            shared_context == (initial.1 || model::pairs_through(
                evidence.spec_reviews(),
                *review,
                *candidate,
                evaluated_at,
                left as nat,
            ).shared_context),
            conflicting_review == (initial.2 || model::pairs_through(
                evidence.spec_reviews(),
                *review,
                *candidate,
                evaluated_at,
                left as nat,
            ).conflicting_review),
        decreases right - left,
    {
        let previous = evidence.reviews()[left];
        if previous.binding().is_current_for(*candidate, evaluated_at) {
            if crate::identity::principal_ids_equal(previous.reviewer(), review.reviewer()) {
                duplicate_reviewer = true;
            }
            if crate::candidate::equality::digests_equal(
                previous.context_digest(),
                review.context_digest(),
            ) {
                shared_context = true;
            }
            if crate::identity::review_ids_equal(previous.id(), review.id())
                && !crate::review::reviews_equal(&previous, review)
            {
                conflicting_review = true;
            }
        }
        proof {
            reveal(model::pairs_through);
            reveal(model::pair_step);
        }
        left += 1;
    }
    (duplicate_reviewer, shared_context, conflicting_review)
}

pub open spec fn counters_satisfied(
    counts: (u16, u16, u16, u16, u16, u16),
    flags: (bool, bool, bool),
) -> bool {
    counts.0 >= super::MIN_INDEPENDENT_REVIEWERS
        && counts.1 == 0
        && counts.2 == 0
        && counts.3 == 0
        && counts.4 == 0
        && counts.5 == 0
        && !flags.0
        && !flags.1
        && !flags.2
}

const fn finish(
    counts: (u16, u16, u16, u16, u16, u16),
    flags: (bool, bool, bool),
) -> (assessment: ReviewAssessment)
    ensures
        assessment.spec_is_satisfied() == counters_satisfied(counts, flags),
        assessment.spec_approved_count() == counts.0,
        assessment.spec_stale_count() == counts.1,
        assessment.spec_mismatched_count() == counts.2,
        assessment.spec_changes_required_count() == counts.3,
        assessment.spec_self_review_count() == counts.4,
        assessment.spec_non_independent_count() == counts.5,
        assessment.spec_duplicate_reviewer() == flags.0,
        assessment.spec_shared_context() == flags.1,
        assessment.spec_conflicting_review() == flags.2,
        assessment.spec_diagnostics_clear() == counters_satisfied(counts, flags),
{
    let satisfied = counts.0 >= super::MIN_INDEPENDENT_REVIEWERS
        && counts.1 == 0
        && counts.2 == 0
        && counts.3 == 0
        && counts.4 == 0
        && counts.5 == 0
        && !flags.0
        && !flags.1
        && !flags.2;
    let assessment = ReviewAssessment::new(
        satisfied,
        counts.0,
        counts.1,
        counts.2,
        counts.3,
        counts.4,
        counts.5,
        flags.0,
        flags.1,
        flags.2,
    );
    proof {
        reveal(counters_satisfied);
        reveal(ReviewAssessment::spec_diagnostics_clear);
    }
    assessment
}

#[allow(clippy::large_types_passed_by_value, reason = "exact candidate identity is a Copy policy value")]
pub(super) fn assess(
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> (assessment: ReviewAssessment)
    ensures
        assessment_matches_reduction(assessment, evidence, candidate, evaluated_at),
        assessment.spec_is_satisfied() == review_satisfied(evidence, candidate, evaluated_at),
        assessment.spec_diagnostics_clear()
            == review_satisfied(evidence, candidate, evaluated_at),
{
    let mut approved_count = 0u16;
    let mut stale_count = 0u16;
    let mut mismatched_count = 0u16;
    let mut changes_required_count = 0u16;
    let mut self_review_count = 0u16;
    let mut non_independent_count = 0u16;
    let mut duplicate_reviewer = false;
    let mut shared_context = false;
    let mut conflicting_review = false;

    let mut right = 0;
    while right < evidence.reviews().len()
        invariant
            0 <= right <= evidence.spec_reviews().len(),
            model::corresponds(
                model::through(
                    evidence.spec_reviews(),
                    candidate,
                    evaluated_at,
                    right as nat,
                ),
                approved_count,
                stale_count,
                mismatched_count,
                changes_required_count,
                self_review_count,
                non_independent_count,
                duplicate_reviewer,
                shared_context,
                conflicting_review,
            ),
        decreases evidence.spec_reviews().len() - right,
    {
        let review = evidence.reviews()[right];
        if review.binding().is_mismatched(candidate) {
            super::increment(&mut mismatched_count);
        } else if review.binding().is_stale_at(candidate, evaluated_at) {
            super::increment(&mut stale_count);
        } else {
            let self_review = crate::identity::principal_ids_equal(
                review.reviewer(),
                review.producer(),
            );
            if self_review { super::increment(&mut self_review_count); }
            if !review.independent_from_producer() {
                super::increment(&mut non_independent_count);
            }
            match review.outcome() {
                ReviewOutcome::Approved => {
                    if !self_review && review.independent_from_producer() {
                        super::increment(&mut approved_count);
                    }
                }
                ReviewOutcome::ChangesRequired => super::increment(&mut changes_required_count),
            }
            let flags = assess_pairs(
                &review,
                right,
                &candidate,
                evaluated_at,
                evidence,
                (duplicate_reviewer, shared_context, conflicting_review),
            );
            duplicate_reviewer = flags.0;
            shared_context = flags.1;
            conflicting_review = flags.2;
        }
        proof {
            reveal(model::through);
            reveal(model::step);
            reveal(model::corresponds);
        }
        right += 1;
    }

    let assessment = finish(
        (
            approved_count,
            stale_count,
            mismatched_count,
            changes_required_count,
            self_review_count,
            non_independent_count,
        ),
        (duplicate_reviewer, shared_context, conflicting_review),
    );
    proof {
        declarative::reduction_matches_inputs(evidence, candidate, evaluated_at);
        reveal(review_satisfied);
        reveal(model::review_satisfied);
        reveal(model::state_satisfied);
        reveal(model::corresponds);
        reveal(counters_satisfied);
        reveal(ReviewAssessment::spec_diagnostics_clear);
        reveal(assessment_matches_reduction);
    }
    assessment
}

} // verus!
