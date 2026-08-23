//! Turn transitions.

use super::AppliedCommand;
use crate::{
    ActionPhase, AttemptPhase, KernelAggregate, KernelCommand, KernelError, KernelErrorKind,
    KernelEventKind, KernelSubject, LifecycleEntity, TurnPhase, TurnState,
};
use peritus_types::{AttemptId, TurnId};
use vstd::prelude::*;

verus! {

pub(super) fn apply(
    state: &mut KernelAggregate,
    command: &KernelCommand,
) -> Result<AppliedCommand, KernelError> {
    match command {
        KernelCommand::StartTurn { attempt_id, turn_id } => start(state, *attempt_id, *turn_id),
        KernelCommand::CompleteTurn { attempt_id, turn_id } => complete(state, *attempt_id, *turn_id),
        KernelCommand::FailTurn { attempt_id, turn_id } => terminate(
            state, *attempt_id, *turn_id, TurnPhase::Failed, ActionPhase::Failed,
            KernelEventKind::TurnFailed,
        ),
        KernelCommand::CancelTurn { attempt_id, turn_id } => terminate(
            state, *attempt_id, *turn_id, TurnPhase::Cancelled, ActionPhase::Cancelled,
            KernelEventKind::TurnCancelled,
        ),
        _ => Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Turn)),
    }
}

fn start(
    state: &mut KernelAggregate,
    attempt_id: AttemptId,
    turn_id: TurnId,
) -> Result<AppliedCommand, KernelError> {
    let Some(attempt_index) = state.attempt_index(attempt_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Attempt));
    };
    if state.attempts[attempt_index].phase() != AttemptPhase::Active {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Attempt));
    }
    if state.turn(turn_id).is_some() {
        return Err(KernelError::entity(KernelErrorKind::DuplicateEntity, LifecycleEntity::Turn));
    }
    if state.has_live_turn_for_attempt(attempt_id) {
        return Err(KernelError::entity(KernelErrorKind::LiveChild, LifecycleEntity::Turn));
    }
    state.turns.push(TurnState::active(turn_id, attempt_id));
    Ok(AppliedCommand::new(KernelEventKind::TurnStarted, KernelSubject::Turn(turn_id)))
}

fn complete(
    state: &mut KernelAggregate,
    attempt_id: AttemptId,
    turn_id: TurnId,
) -> Result<AppliedCommand, KernelError> {
    let Some(index) = state.turn_index(turn_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Turn));
    };
    if state.turns[index].attempt_id() != attempt_id {
        return Err(KernelError::entity(KernelErrorKind::ParentMismatch, LifecycleEntity::Turn));
    }
    if state.turns[index].phase() != TurnPhase::Active {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Turn));
    }
    if state.has_live_action_for_turn(turn_id) {
        return Err(KernelError::entity(KernelErrorKind::LiveChild, LifecycleEntity::Action));
    }
    state.turns[index].set_phase(TurnPhase::Completed);
    Ok(AppliedCommand::new(KernelEventKind::TurnCompleted, KernelSubject::Turn(turn_id)))
}

fn terminate(
    state: &mut KernelAggregate,
    attempt_id: AttemptId,
    turn_id: TurnId,
    turn_phase: TurnPhase,
    action_phase: ActionPhase,
    event_kind: KernelEventKind,
) -> Result<AppliedCommand, KernelError> {
    let Some(index) = state.turn_index(turn_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Turn));
    };
    if state.turns[index].attempt_id() != attempt_id {
        return Err(KernelError::entity(KernelErrorKind::ParentMismatch, LifecycleEntity::Turn));
    }
    if state.turns[index].phase() != TurnPhase::Active {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Turn));
    }
    let mut action_index = 0;
    while action_index < state.actions.len()
        invariant
            action_index <= state.actions.len(),
            (index as int) < state.turns@.len(),
        decreases state.actions.len() - action_index,
    {
        if state.actions[action_index].turn_id() == turn_id
            && !state.actions[action_index].phase().is_terminal()
        {
            state.actions[action_index].set_phase(action_phase);
        }
        action_index += 1;
    }
    state.turns[index].set_phase(turn_phase);
    Ok(AppliedCommand::new(event_kind, KernelSubject::Turn(turn_id)))
}

} // verus!
