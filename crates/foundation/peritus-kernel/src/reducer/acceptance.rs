//! Exact-current-evidence acceptance transitions.

use super::AppliedCommand;
use crate::{
    AcceptanceOutcome, AcceptancePhase, AttemptPhase, AuthorityInputKind, KernelAggregate,
    KernelCommand, KernelError, KernelErrorKind, KernelEventKind, KernelSubject, LifecycleEntity,
    ReducerInputs, ReviewPhase, RunPhase, WaiverPhase,
};
use peritus_quality_policy::{AcceptanceEvidence, evaluate_acceptance};
use peritus_types::{AttemptId, RunId};
use vstd::prelude::*;

verus! {

pub(super) fn apply(
    state: &mut KernelAggregate,
    command: &KernelCommand,
    inputs: &ReducerInputs<'_>,
) -> Result<AppliedCommand, KernelError> {
    match command {
        KernelCommand::BeginAcceptance { run_id } => begin(state, *run_id),
        KernelCommand::EvaluateAcceptance { run_id } => evaluate(state, *run_id, inputs),
        _ => Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Acceptance)),
    }
}

fn begin(state: &mut KernelAggregate, run_id: RunId) -> Result<AppliedCommand, KernelError> {
    let Some(run_index) = state.run_index(run_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Run));
    };
    let Some(attempt_id) = state.runs[run_index].current_attempt_id() else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Attempt));
    };
    let Some(attempt_index) = state.attempt_index(attempt_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Attempt));
    };
    if state.runs[run_index].phase() != RunPhase::Reviewing
        || state.runs[run_index].acceptance() != AcceptancePhase::Pending
        || state.attempts[attempt_index].phase() != AttemptPhase::Reviewing
        || !has_submitted_review(state, run_id, attempt_id)
    {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Acceptance));
    }
    state.runs[run_index].set_acceptance(AcceptancePhase::Evaluating);
    Ok(AppliedCommand::new(
        KernelEventKind::AcceptanceBegun,
        KernelSubject::Acceptance(run_id),
    ))
}

fn evaluate(
    state: &mut KernelAggregate,
    run_id: RunId,
    inputs: &ReducerInputs<'_>,
) -> Result<AppliedCommand, KernelError> {
    let Some(run_index) = state.run_index(run_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Run));
    };
    let Some(attempt_id) = state.runs[run_index].current_attempt_id() else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Attempt));
    };
    let Some(attempt_index) = state.attempt_index(attempt_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Attempt));
    };
    if state.runs[run_index].phase() != RunPhase::Reviewing
        || state.runs[run_index].acceptance() != AcceptancePhase::Evaluating
        || state.attempts[attempt_index].phase() != AttemptPhase::Reviewing
    {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Acceptance));
    }
    let Some(evidence) = inputs.acceptance_evidence() else {
        return Err(KernelError::authority(
            KernelErrorKind::MissingAuthorityInput,
            AuthorityInputKind::AcceptanceEvidence,
        ));
    };
    if !evidence_matches_projection(state, run_id, attempt_id, evidence) {
        return Err(KernelError::authority(
            KernelErrorKind::AuthorityMismatch,
            AuthorityInputKind::AcceptanceEvidence,
        ));
    }
    let decision = evaluate_acceptance(inputs.contract(), state.revision, evidence);
    if decision.is_acceptable() {
        state.attempts[attempt_index].set_phase(AttemptPhase::Accepted);
        state.runs[run_index].set_phase(RunPhase::Accepted);
        state.runs[run_index].set_acceptance(AcceptancePhase::Accepted);
        state.runs[run_index].set_current_attempt(None);
        Ok(AppliedCommand::acceptance(
            KernelEventKind::AcceptanceAccepted,
            KernelSubject::Acceptance(run_id),
            AcceptanceOutcome::Accepted,
        ))
    } else {
        let unmet_conditions = decision.unmet_conditions().len();
        state.attempts[attempt_index].set_phase(AttemptPhase::Fixing);
        state.runs[run_index].set_phase(RunPhase::Fixing);
        state.runs[run_index].set_acceptance(AcceptancePhase::NeedsChanges);
        Ok(AppliedCommand::acceptance(
            KernelEventKind::AcceptanceNeedsChanges,
            KernelSubject::Acceptance(run_id),
            AcceptanceOutcome::NeedsChanges { unmet_conditions },
        ))
    }
}

fn has_submitted_review(state: &KernelAggregate, run_id: RunId, attempt_id: AttemptId) -> bool {
    let mut index = 0;
    while index < state.reviews.len()
        invariant index <= state.reviews.len(),
        decreases state.reviews.len() - index,
    {
        let review = state.reviews[index];
        if review.run_id() == run_id
            && review.attempt_id() == attempt_id
            && review.phase() == ReviewPhase::Submitted
        {
            return true;
        }
        index += 1;
    }
    false
}

fn evidence_matches_projection(
    state: &KernelAggregate,
    run_id: RunId,
    attempt_id: AttemptId,
    evidence: &AcceptanceEvidence,
) -> bool {
    let mut index = 0;
    while index < evidence.reviews().len()
        invariant index <= evidence.spec_reviews().len(),
        decreases evidence.spec_reviews().len() - index,
    {
        let observation = &evidence.reviews()[index];
        let Some(review) = state.review(observation.cycle_id()) else { return false; };
        if review.run_id() != run_id
            || review.attempt_id() != attempt_id
            || review.phase() != ReviewPhase::Submitted
            || observation.revision() != state.revision
        {
            return false;
        }
        index += 1;
    }
    let mut index = 0;
    while index < evidence.waivers().len()
        invariant index <= evidence.spec_waivers().len(),
        decreases evidence.spec_waivers().len() - index,
    {
        let observation = &evidence.waivers()[index];
        let Some(waiver) = state.waiver(observation.finding_id()) else { return false; };
        if waiver.run_id() != run_id
            || waiver.phase() != WaiverPhase::Granted
            || observation.revision() != state.revision
        {
            return false;
        }
        index += 1;
    }
    true
}

} // verus!
