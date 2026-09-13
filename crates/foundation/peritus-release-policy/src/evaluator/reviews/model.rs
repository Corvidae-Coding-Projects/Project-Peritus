//! Exact finite reduction model for supplied independent reviews.

use crate::{ReleaseCandidate, ReviewObservation, ReviewOutcome};
use vstd::prelude::*;

verus! {

pub struct PairFlags {
    pub duplicate_reviewer: bool,
    pub shared_context: bool,
    pub conflicting_review: bool,
}

pub struct ReviewState {
    pub approved_count: u16,
    pub stale_count: u16,
    pub mismatched_count: u16,
    pub changes_required_count: u16,
    pub self_review_count: u16,
    pub non_independent_count: u16,
    pub duplicate_reviewer: bool,
    pub shared_context: bool,
    pub conflicting_review: bool,
}

pub open spec fn initial_pairs() -> PairFlags {
    PairFlags {
        duplicate_reviewer: false,
        shared_context: false,
        conflicting_review: false,
    }
}

pub open spec fn initial() -> ReviewState {
    ReviewState {
        approved_count: 0,
        stale_count: 0,
        mismatched_count: 0,
        changes_required_count: 0,
        self_review_count: 0,
        non_independent_count: 0,
        duplicate_reviewer: false,
        shared_context: false,
        conflicting_review: false,
    }
}

pub open spec fn pair_step(
    flags: PairFlags,
    previous: ReviewObservation,
    current: ReviewObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> PairFlags {
    if previous.spec_binding().spec_is_current_for(candidate, evaluated_at) {
        PairFlags {
            duplicate_reviewer: flags.duplicate_reviewer
                || crate::identity::principal_ids_match(
                    previous.spec_reviewer(),
                    current.spec_reviewer(),
                ),
            shared_context: flags.shared_context
                || crate::candidate::digest_matches(
                    previous.spec_context_digest(),
                    current.spec_context_digest(),
                ),
            conflicting_review: flags.conflicting_review
                || (crate::identity::review_ids_match(previous.spec_id(), current.spec_id())
                    && !previous.spec_matches(current)),
        }
    } else {
        flags
    }
}

pub open spec fn pairs_through(
    values: Seq<ReviewObservation>,
    current: ReviewObservation,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
) -> PairFlags
    decreases end,
{
    if end == 0 {
        initial_pairs()
    } else {
        pair_step(
            pairs_through(values, current, candidate, evaluated_at, (end - 1) as nat),
            values[(end - 1) as int],
            current,
            candidate,
            evaluated_at,
        )
    }
}

pub open spec fn step(
    state: ReviewState,
    values: Seq<ReviewObservation>,
    index: nat,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> ReviewState {
    let review = values[index as int];
    if review.spec_binding().spec_is_mismatched(candidate) {
        ReviewState {
            mismatched_count: super::super::saturated_increment(state.mismatched_count),
            ..state
        }
    } else if review.spec_binding().spec_is_stale_at(candidate, evaluated_at) {
        ReviewState {
            stale_count: super::super::saturated_increment(state.stale_count),
            ..state
        }
    } else {
        let self_review = crate::identity::principal_ids_match(
            review.spec_reviewer(),
            review.spec_producer(),
        );
        let independent = review.spec_independent_from_producer();
        let approved = review.spec_outcome() == ReviewOutcome::Approved
            && !self_review
            && independent;
        let changes_required = review.spec_outcome() == ReviewOutcome::ChangesRequired;
        let pairs = pairs_through(values, review, candidate, evaluated_at, index);
        ReviewState {
            approved_count: if approved {
                super::super::saturated_increment(state.approved_count)
            } else {
                state.approved_count
            },
            changes_required_count: if changes_required {
                super::super::saturated_increment(state.changes_required_count)
            } else {
                state.changes_required_count
            },
            self_review_count: if self_review {
                super::super::saturated_increment(state.self_review_count)
            } else {
                state.self_review_count
            },
            non_independent_count: if independent {
                state.non_independent_count
            } else {
                super::super::saturated_increment(state.non_independent_count)
            },
            duplicate_reviewer: state.duplicate_reviewer || pairs.duplicate_reviewer,
            shared_context: state.shared_context || pairs.shared_context,
            conflicting_review: state.conflicting_review || pairs.conflicting_review,
            ..state
        }
    }
}

pub open spec fn through(
    values: Seq<ReviewObservation>,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
    end: nat,
) -> ReviewState
    decreases end,
{
    if end == 0 {
        initial()
    } else {
        step(
            through(values, candidate, evaluated_at, (end - 1) as nat),
            values,
            (end - 1) as nat,
            candidate,
            evaluated_at,
        )
    }
}

pub open spec fn state_satisfied(state: ReviewState) -> bool {
    state.approved_count >= super::super::MIN_INDEPENDENT_REVIEWERS
        && state.stale_count == 0
        && state.mismatched_count == 0
        && state.changes_required_count == 0
        && state.self_review_count == 0
        && state.non_independent_count == 0
        && !state.duplicate_reviewer
        && !state.shared_context
        && !state.conflicting_review
}

pub open spec fn review_satisfied(
    values: Seq<ReviewObservation>,
    candidate: ReleaseCandidate,
    evaluated_at: u64,
) -> bool {
    state_satisfied(through(values, candidate, evaluated_at, values.len() as nat))
}

pub open spec fn corresponds(
    state: ReviewState,
    approved_count: u16,
    stale_count: u16,
    mismatched_count: u16,
    changes_required_count: u16,
    self_review_count: u16,
    non_independent_count: u16,
    duplicate_reviewer: bool,
    shared_context: bool,
    conflicting_review: bool,
) -> bool {
    state.approved_count == approved_count
        && state.stale_count == stale_count
        && state.mismatched_count == mismatched_count
        && state.changes_required_count == changes_required_count
        && state.self_review_count == self_review_count
        && state.non_independent_count == non_independent_count
        && state.duplicate_reviewer == duplicate_reviewer
        && state.shared_context == shared_context
        && state.conflicting_review == conflicting_review
}

} // verus!
