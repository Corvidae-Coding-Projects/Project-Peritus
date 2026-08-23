//! Review-cycle transitions.

use super::AppliedCommand;
use crate::{
    AttemptPhase, KernelAggregate, KernelCommand, KernelError, KernelErrorKind, KernelEventKind,
    KernelSubject, LifecycleEntity, ReducerInputs, ReviewPhase, ReviewState, RunPhase,
};
use peritus_types::{AttemptId, ReviewCycleId, RunId};
use vstd::prelude::*;

verus! {

pub(super) fn apply(
    state: &mut KernelAggregate,
    command: &KernelCommand,
    inputs: &ReducerInputs<'_>,
) -> Result<AppliedCommand, KernelError> {
    match command {
        KernelCommand::RequestReview { run_id, attempt_id, review_id } => {
            request(state, *run_id, *attempt_id, *review_id, inputs)
        }
        KernelCommand::BeginReview { review_id } => phase(
            state, *review_id, ReviewPhase::Requested, ReviewPhase::Active,
            KernelEventKind::ReviewBegun,
        ),
        KernelCommand::SubmitReview { review_id } => phase(
            state, *review_id, ReviewPhase::Active, ReviewPhase::Submitted,
            KernelEventKind::ReviewSubmitted,
        ),
        KernelCommand::InvalidateReview { review_id } => invalidate(state, *review_id),
        _ => Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Review)),
    }
}

fn request(
    state: &mut KernelAggregate,
    run_id: RunId,
    attempt_id: AttemptId,
    review_id: ReviewCycleId,
    inputs: &ReducerInputs<'_>,
) -> Result<AppliedCommand, KernelError> {
    let (Some(run_index), Some(attempt_index)) = (state.run_index(run_id), state.attempt_index(attempt_id)) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Attempt));
    };
    if state.attempts[attempt_index].run_id() != run_id {
        return Err(KernelError::entity(KernelErrorKind::ParentMismatch, LifecycleEntity::Attempt));
    }
    if state.runs[run_index].phase() != RunPhase::Reviewing
        || !matches!(state.attempts[attempt_index].phase(), AttemptPhase::Submitted | AttemptPhase::Reviewing)
    {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Review));
    }
    if state.review(review_id).is_some() {
        return Err(KernelError::entity(KernelErrorKind::DuplicateEntity, LifecycleEntity::Review));
    }
    let mut count = 0usize;
    let mut index = 0;
    while index < state.reviews.len()
        invariant index <= state.reviews.len(), count <= index,
        decreases state.reviews.len() - index,
    {
        if state.reviews[index].run_id() == run_id { count += 1; }
        index += 1;
    }
    if count >= usize::from(inputs.contract().completion_policy().max_review_cycles()) {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Review));
    }
    state.reviews.push(ReviewState::requested(review_id, run_id, attempt_id));
    state.attempts[attempt_index].set_phase(AttemptPhase::Reviewing);
    Ok(AppliedCommand::new(
        KernelEventKind::ReviewRequested,
        KernelSubject::Review(review_id),
    ))
}

fn phase(
    state: &mut KernelAggregate,
    review_id: ReviewCycleId,
    expected: ReviewPhase,
    next: ReviewPhase,
    event_kind: KernelEventKind,
) -> Result<AppliedCommand, KernelError> {
    let Some(index) = state.review_index(review_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Review));
    };
    if state.reviews[index].phase() != expected {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Review));
    }
    state.reviews[index].set_phase(next);
    Ok(AppliedCommand::new(event_kind, KernelSubject::Review(review_id)))
}

fn invalidate(
    state: &mut KernelAggregate,
    review_id: ReviewCycleId,
) -> Result<AppliedCommand, KernelError> {
    let Some(index) = state.review_index(review_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Review));
    };
    if state.reviews[index].phase() == ReviewPhase::Invalidated {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Review));
    }
    state.reviews[index].set_phase(ReviewPhase::Invalidated);
    Ok(AppliedCommand::new(
        KernelEventKind::ReviewInvalidated,
        KernelSubject::Review(review_id),
    ))
}

} // verus!
