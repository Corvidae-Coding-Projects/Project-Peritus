//! Independent-review reduction over the supplied observations.

use crate::{
    ReleaseCandidate, ReleaseEvidence, ReviewAssessment, ReviewObservation, ReviewOutcome,
};

fn assess_pairs(
    review: &ReviewObservation,
    right: usize,
    candidate: &ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
    initial: (bool, bool, bool),
) -> (bool, bool, bool) {
    let mut duplicate_reviewer = initial.0;
    let mut shared_context = initial.1;
    let mut conflicting_review = initial.2;
    let mut left = 0;
    while left < right {
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
        left += 1;
    }
    (duplicate_reviewer, shared_context, conflicting_review)
}
const fn finish(
    counts: (u16, u16, u16, u16, u16, u16),
    flags: (bool, bool, bool),
) -> ReviewAssessment {
    let satisfied = counts.0 >= super::MIN_INDEPENDENT_REVIEWERS
        && counts.1 == 0
        && counts.2 == 0
        && counts.3 == 0
        && counts.4 == 0
        && counts.5 == 0
        && !flags.0
        && !flags.1
        && !flags.2;

    ReviewAssessment::new(
        satisfied, counts.0, counts.1, counts.2, counts.3, counts.4, counts.5, flags.0, flags.1,
        flags.2,
    )
}
#[allow(
    clippy::large_types_passed_by_value,
    reason = "exact candidate identity is a Copy policy value"
)]
pub(super) fn assess(
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    evidence: &ReleaseEvidence,
) -> ReviewAssessment {
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
    while right < evidence.reviews().len() {
        let review = evidence.reviews()[right];
        if review.binding().is_mismatched(candidate) {
            super::increment(&mut mismatched_count);
        } else if review.binding().is_stale_at(candidate, evaluated_at) {
            super::increment(&mut stale_count);
        } else {
            let self_review =
                crate::identity::principal_ids_equal(review.reviewer(), review.producer());
            if self_review {
                super::increment(&mut self_review_count);
            }
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
        right += 1;
    }

    finish(
        (
            approved_count,
            stale_count,
            mismatched_count,
            changes_required_count,
            self_review_count,
            non_independent_count,
        ),
        (duplicate_reviewer, shared_context, conflicting_review),
    )
}
