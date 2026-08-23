//! Action proposal, authorization, dispatch, and completion transitions.

use super::AppliedCommand;
use crate::{
    ActionAuthorizationWitness, ActionPhase, ActionState, AuthorityInputKind, KernelAggregate,
    KernelCommand, KernelError, KernelErrorKind, KernelEventKind, KernelSubject, LifecycleEntity,
    ReducerInputs, TurnPhase,
};
use peritus_policy::ActorRole;
use peritus_types::{ActionId, ActorId, EnvironmentId, Sha256Digest, TurnId};
use vstd::prelude::*;

verus! {

pub(super) fn apply(
    state: &mut KernelAggregate,
    command: &KernelCommand,
    inputs: &ReducerInputs<'_>,
) -> Result<AppliedCommand, KernelError> {
    match command {
        KernelCommand::ProposeAction {
            turn_id, action_id, digest, actor_id, role, environment_id,
        } => propose(state, *turn_id, *action_id, *digest, *actor_id, *role, *environment_id),
        KernelCommand::AuthorizeAction { action_id } => authorize(state, *action_id, inputs),
        KernelCommand::DispatchAction { action_id } => phase(
            state, *action_id, ActionPhase::Authorized, ActionPhase::Dispatched,
            KernelEventKind::ActionDispatched,
        ),
        KernelCommand::CompleteAction { action_id } => phase(
            state, *action_id, ActionPhase::Dispatched, ActionPhase::Succeeded,
            KernelEventKind::ActionCompleted,
        ),
        KernelCommand::FailAction { action_id } => fail(state, *action_id),
        KernelCommand::CancelAction { action_id } => cancel(state, *action_id),
        _ => Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Action)),
    }
}

#[allow(clippy::too_many_arguments)]
fn propose(
    state: &mut KernelAggregate,
    turn_id: TurnId,
    action_id: ActionId,
    digest: Sha256Digest,
    actor_id: ActorId,
    role: ActorRole,
    environment_id: EnvironmentId,
) -> Result<AppliedCommand, KernelError> {
    let Some(turn_index) = state.turn_index(turn_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Turn));
    };
    if state.turns[turn_index].phase() != TurnPhase::Active {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Turn));
    }
    if state.action(action_id).is_some() {
        return Err(KernelError::entity(KernelErrorKind::DuplicateEntity, LifecycleEntity::Action));
    }
    state.actions.push(ActionState::proposed(
        action_id, turn_id, digest, actor_id, role, environment_id,
    ));
    Ok(AppliedCommand::new(
        KernelEventKind::ActionProposed,
        KernelSubject::Action(action_id),
    ))
}

fn authorize(
    state: &mut KernelAggregate,
    action_id: ActionId,
    inputs: &ReducerInputs<'_>,
) -> Result<AppliedCommand, KernelError> {
    let Some(index) = state.action_index(action_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Action));
    };
    if state.actions[index].phase() != ActionPhase::Proposed {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Action));
    }
    let Some(transition) = inputs.capability_use() else {
        return Err(KernelError::authority(
            KernelErrorKind::MissingAuthorityInput,
            AuthorityInputKind::CapabilityUse,
        ));
    };
    let witness = ActionAuthorizationWitness::from_transition(
        action_id,
        state.actions[index].digest(),
        state.actions[index].actor_id(),
        state.actions[index].role(),
        state.actions[index].environment_id(),
        state.revision,
        transition,
    )?;
    state.actions[index].authorize(witness);
    Ok(AppliedCommand::new(
        KernelEventKind::ActionAuthorized,
        KernelSubject::Action(action_id),
    ))
}

fn phase(
    state: &mut KernelAggregate,
    action_id: ActionId,
    expected: ActionPhase,
    next: ActionPhase,
    event_kind: KernelEventKind,
) -> Result<AppliedCommand, KernelError> {
    let Some(index) = state.action_index(action_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Action));
    };
    if state.actions[index].phase() != expected {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Action));
    }
    state.actions[index].set_phase(next);
    Ok(AppliedCommand::new(event_kind, KernelSubject::Action(action_id)))
}

fn fail(state: &mut KernelAggregate, action_id: ActionId) -> Result<AppliedCommand, KernelError> {
    let Some(index) = state.action_index(action_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Action));
    };
    if !matches!(state.actions[index].phase(), ActionPhase::Authorized | ActionPhase::Dispatched) {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Action));
    }
    state.actions[index].set_phase(ActionPhase::Failed);
    Ok(AppliedCommand::new(KernelEventKind::ActionFailed, KernelSubject::Action(action_id)))
}

fn cancel(state: &mut KernelAggregate, action_id: ActionId) -> Result<AppliedCommand, KernelError> {
    let Some(index) = state.action_index(action_id) else {
        return Err(KernelError::entity(KernelErrorKind::MissingEntity, LifecycleEntity::Action));
    };
    if !matches!(state.actions[index].phase(), ActionPhase::Proposed | ActionPhase::Authorized) {
        return Err(KernelError::entity(KernelErrorKind::IllegalPhase, LifecycleEntity::Action));
    }
    state.actions[index].set_phase(ActionPhase::Cancelled);
    Ok(AppliedCommand::new(KernelEventKind::ActionCancelled, KernelSubject::Action(action_id)))
}

} // verus!
