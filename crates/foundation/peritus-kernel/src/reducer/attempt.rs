//! Attempt admission and candidate-submission transitions.

use super::AppliedCommand;
use crate::{
    AcceptancePhase, ActionPhase, AttemptPhase, AttemptState, AuthorityInputKind, KernelAggregate,
    KernelCommand, KernelError, KernelErrorKind, KernelEventKind, KernelSubject, LifecycleEntity,
    ReducerInputs, RunPhase, SessionPhase, TurnPhase,
};
use peritus_budget::BudgetAccountPhase;
use peritus_types::{AttemptId, RunId};
use vstd::prelude::*;

verus! {

pub(super) fn apply(
    state: &mut KernelAggregate,
    command: &KernelCommand,
    inputs: &ReducerInputs<'_>,
) -> Result<AppliedCommand, KernelError> {
    match command {
        KernelCommand::StartAttempt { run_id, attempt_id } => start(state, *run_id, *attempt_id, inputs),
        KernelCommand::ResumeAttempt { run_id, attempt_id } => resume(state, *run_id, *attempt_id),
        KernelCommand::SubmitAttempt { run_id, attempt_id } => submit(state, *run_id, *attempt_id),
        KernelCommand::FailAttempt { run_id, attempt_id } => terminate(
            state, *run_id, *attempt_id, AttemptPhase::Failed,
            TurnPhase::Failed, ActionPhase::Failed, KernelEventKind::AttemptFailed,
        ),
        KernelCommand::ExhaustAttempt { run_id, attempt_id } => terminate(
            state, *run_id, *attempt_id, AttemptPhase::Exhausted,
            TurnPhase::Cancelled, ActionPhase::Cancelled, KernelEventKind::AttemptExhausted,
        ),
        _ => Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Attempt)),
    }
}

fn start(
    state: &mut KernelAggregate,
    run_id: RunId,
    attempt_id: AttemptId,
    inputs: &ReducerInputs<'_>,
) -> Result<AppliedCommand, KernelError> {
    if state.session.phase() != SessionPhase::Open {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Session));
    }
    let Some(run_index) = state.run_index(run_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Run));
    };
    if !matches!(state.runs[run_index].phase(), RunPhase::Pending | RunPhase::Running)
        || state.runs[run_index].current_attempt_id().is_some()
    {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Run));
    }
    if state.attempt(attempt_id).is_some() {
        return Err(KernelError::entity(KernelErrorKind::DuplicateEntity, LifecycleEntity::Attempt));
    }
    let Some(child) = inputs.attempt_budget() else {
        return Err(KernelError::authority(
            KernelErrorKind::MissingAuthorityInput, AuthorityInputKind::AttemptBudget,
        ));
    };
    let Some(parent) = inputs.parent_budget() else {
        return Err(KernelError::authority(
            KernelErrorKind::MissingAuthorityInput, AuthorityInputKind::ParentBudget,
        ));
    };
    if child.revision() != state.revision
        || parent.revision() != state.revision
        || child.parent_id() != Some(parent.id())
        || parent.id() != state.runs[run_index].budget_id()
    {
        return Err(KernelError::authority(
            KernelErrorKind::AuthorityMismatch, AuthorityInputKind::AttemptBudget,
        ));
    }
    if child.phase() != BudgetAccountPhase::Open || parent.phase() != BudgetAccountPhase::Open {
        return Err(KernelError::authority(
            KernelErrorKind::BudgetUnavailable, AuthorityInputKind::AttemptBudget,
        ));
    }
    if !child.limits().amounts().fits_within(parent.available()) {
        return Err(KernelError::authority(
            KernelErrorKind::BudgetExceeded, AuthorityInputKind::AttemptBudget,
        ));
    }
    state.attempts.push(AttemptState::active(
        attempt_id, run_id, child.id(), child.limits(),
    ));
    state.runs[run_index].set_phase(RunPhase::Running);
    state.runs[run_index].set_acceptance(AcceptancePhase::Pending);
    state.runs[run_index].set_current_attempt(Some(attempt_id));
    Ok(AppliedCommand::new(
        KernelEventKind::AttemptStarted,
        KernelSubject::Attempt(attempt_id),
    ))
}

fn resume(
    state: &mut KernelAggregate,
    run_id: RunId,
    attempt_id: AttemptId,
) -> Result<AppliedCommand, KernelError> {
    let (Some(run_index), Some(attempt_index)) = (state.run_index(run_id), state.attempt_index(attempt_id)) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Attempt));
    };
    if state.attempts[attempt_index].run_id() != run_id {
        return Err(KernelError::entity(KernelErrorKind::ParentMismatch, LifecycleEntity::Attempt));
    }
    if state.runs[run_index].phase() != RunPhase::Fixing
        || state.attempts[attempt_index].phase() != AttemptPhase::Fixing
        || state.has_live_turn_for_attempt(attempt_id)
    {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Attempt));
    }
    state.attempts[attempt_index].set_phase(AttemptPhase::Active);
    state.runs[run_index].set_phase(RunPhase::Running);
    state.runs[run_index].set_acceptance(AcceptancePhase::Pending);
    Ok(AppliedCommand::new(
        KernelEventKind::AttemptResumed,
        KernelSubject::Attempt(attempt_id),
    ))
}

fn submit(
    state: &mut KernelAggregate,
    run_id: RunId,
    attempt_id: AttemptId,
) -> Result<AppliedCommand, KernelError> {
    let (Some(run_index), Some(attempt_index)) = (state.run_index(run_id), state.attempt_index(attempt_id)) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Attempt));
    };
    if state.attempts[attempt_index].run_id() != run_id {
        return Err(KernelError::entity(KernelErrorKind::ParentMismatch, LifecycleEntity::Attempt));
    }
    if state.runs[run_index].phase() != RunPhase::Running
        || state.attempts[attempt_index].phase() != AttemptPhase::Active
        || state.has_live_turn_for_attempt(attempt_id)
    {
        return Err(KernelError::entity(KernelErrorKind::LiveChild, LifecycleEntity::Turn));
    }
    state.attempts[attempt_index].set_phase(AttemptPhase::Submitted);
    state.runs[run_index].set_phase(RunPhase::Reviewing);
    state.runs[run_index].set_acceptance(AcceptancePhase::Pending);
    Ok(AppliedCommand::new(
        KernelEventKind::AttemptSubmitted,
        KernelSubject::Attempt(attempt_id),
    ))
}

#[allow(
    clippy::option_if_let_else,
    reason = "explicit parent traversal mirrors the Verus lifecycle relation"
)]
fn terminate(
    state: &mut KernelAggregate,
    run_id: RunId,
    attempt_id: AttemptId,
    attempt_phase: AttemptPhase,
    turn_phase: TurnPhase,
    action_phase: ActionPhase,
    event_kind: KernelEventKind,
) -> Result<AppliedCommand, KernelError> {
    let (Some(run_index), Some(attempt_index)) = (state.run_index(run_id), state.attempt_index(attempt_id)) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Attempt));
    };
    if state.attempts[attempt_index].run_id() != run_id {
        return Err(KernelError::entity(KernelErrorKind::ParentMismatch, LifecycleEntity::Attempt));
    }
    if state.runs[run_index].phase().is_terminal() || state.attempts[attempt_index].phase().is_terminal() {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Attempt));
    }
    let mut turn_index = 0;
    while turn_index < state.turns.len()
        invariant
            turn_index <= state.turns.len(),
            (run_index as int) < state.runs@.len(),
            (attempt_index as int) < state.attempts@.len(),
        decreases state.turns.len() - turn_index,
    {
        if state.turns[turn_index].attempt_id() == attempt_id
            && !state.turns[turn_index].phase().is_terminal()
        {
            state.turns[turn_index].set_phase(turn_phase);
        }
        turn_index += 1;
    }
    let mut action_index = 0;
    while action_index < state.actions.len()
        invariant
            action_index <= state.actions.len(),
            (run_index as int) < state.runs@.len(),
            (attempt_index as int) < state.attempts@.len(),
        decreases state.actions.len() - action_index,
    {
        let belongs = match state.turn(state.actions[action_index].turn_id()) {
            Some(turn) => turn.attempt_id() == attempt_id,
            None => false,
        };
        if belongs && !state.actions[action_index].phase().is_terminal() {
            state.actions[action_index].set_phase(action_phase);
        }
        action_index += 1;
    }
    state.attempts[attempt_index].set_phase(attempt_phase);
    state.runs[run_index].set_phase(RunPhase::Running);
    state.runs[run_index].set_acceptance(AcceptancePhase::Pending);
    state.runs[run_index].set_current_attempt(None);
    Ok(AppliedCommand::new(event_kind, KernelSubject::Attempt(attempt_id)))
}

} // verus!
