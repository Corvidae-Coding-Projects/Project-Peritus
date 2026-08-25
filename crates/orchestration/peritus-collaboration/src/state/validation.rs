//! Inert checkpoint validation before comparison with authoritative replay.

use crate::error::{CollaborationError, CollaborationErrorKind, reject};
use crate::{CollaborationPhase, CollaborationState, TaskPhase};

pub(super) fn validate(state: &CollaborationState) -> Result<(), CollaborationError> {
    state.binding().validate()?;
    validate_collections(state)?;
    let root = state.task(state.binding().root_task_id()).ok_or_else(|| {
        reject(CollaborationErrorKind::CausalityViolation, "decoded state has no root task")
    })?;
    if root.assignment() != state.binding().root_assignment() {
        return Err(reject(
            CollaborationErrorKind::BindingMismatch,
            "decoded state root differs from immutable binding",
        ));
    }
    for task in state.tasks() {
        validate_task(state, task)?;
    }
    for delivery in state.messages() {
        let message = delivery.message();
        if message.root_task_id() != state.binding().root_task_id()
            || message.revision() != state.binding().revision()
            || message.payload_bytes() > state.limits().payload_bytes()
            || state.task(message.task_id()).is_none()
            || message.predecessor().is_some_and(|id| {
                state.message(id).is_none_or(|predecessor| {
                    predecessor.message().task_id() != message.task_id()
                        || predecessor.message().ordinal().saturating_add(1) != message.ordinal()
                })
            })
        {
            return Err(reject(
                CollaborationErrorKind::CausalityViolation,
                "decoded message root, task, predecessor, payload, or revision is invalid",
            ));
        }
    }
    if (state.phase() == CollaborationPhase::Terminal) != state.terminal().is_some()
        || state.terminal().is_some_and(|terminal| {
            crate::canonical::terminal_digest(terminal) != terminal.digest()
        })
        || crate::canonical::state_digest(state) != state.state_digest()
    {
        return Err(reject(
            CollaborationErrorKind::BindingMismatch,
            "decoded aggregate terminal phase or digest is inconsistent",
        ));
    }
    Ok(())
}

fn validate_collections(state: &CollaborationState) -> Result<(), CollaborationError> {
    if state.tasks().is_empty()
        || state.tasks().len() > state.limits().tasks() as usize
        || state.messages().len() > state.limits().messages() as usize
        || state.used_commands().is_empty()
        || usize::try_from(state.sequence().get()).ok() != Some(state.used_commands().len())
        || state.estimated_encoded_bytes() > state.limits().state_bytes()
        || state
            .tasks()
            .windows(2)
            .any(|pair| pair[0].assignment().task_id() >= pair[1].assignment().task_id())
        || state.messages().windows(2).any(|pair| pair[0].message().id() >= pair[1].message().id())
        || state
            .used_commands()
            .iter()
            .enumerate()
            .any(|(index, id)| state.used_commands()[..index].contains(id))
    {
        Err(reject(
            CollaborationErrorKind::NonCanonical,
            "decoded collaboration state exceeds limits or contains noncanonical identities",
        ))
    } else {
        Ok(())
    }
}

fn validate_task(
    state: &CollaborationState,
    task: &crate::CollaborationTask,
) -> Result<(), CollaborationError> {
    let assignment = task.assignment();
    if assignment.root_task_id() != state.binding().root_task_id()
        || assignment.depth() > state.limits().depth()
        || state
            .tasks()
            .iter()
            .filter(|other| other.assignment().work_id() == assignment.work_id())
            .count()
            != 1
    {
        return Err(reject(
            CollaborationErrorKind::CausalityViolation,
            "decoded task root, depth, or scheduler work binding is invalid",
        ));
    }
    if let Some(parent_id) = assignment.parent_task_id() {
        let parent = state.task(parent_id).ok_or_else(|| {
            reject(CollaborationErrorKind::CausalityViolation, "decoded task parent is absent")
        })?;
        if assignment.depth() != parent.assignment().depth().saturating_add(1)
            || assignment.parent_owner() != parent.assignment().owner()
            || state.children(parent_id).len() > usize::from(state.limits().fan_out())
        {
            return Err(reject(
                CollaborationErrorKind::CausalityViolation,
                "decoded task parent, depth, owner, or fan-out is invalid",
            ));
        }
    }
    if (matches!(task.phase(), TaskPhase::Active | TaskPhase::Cancelling)
        && task.reservation().is_none())
        || (matches!(task.phase(), TaskPhase::Offered | TaskPhase::Accepted)
            && task.reservation().is_some())
        || (task.phase() == TaskPhase::Terminal) != task.terminal().is_some()
        || task.reservation().is_some_and(|reservation| {
            reservation.work_id() != assignment.work_id()
                || reservation.owner() != assignment.owner()
                || reservation.revision() != state.binding().revision()
        })
        || task
            .terminal()
            .and_then(crate::TaskTerminal::handoff)
            .is_some_and(|handoff| handoff.revision() != state.binding().revision())
    {
        return Err(reject(
            CollaborationErrorKind::BindingMismatch,
            "decoded task lifecycle, reservation, terminal, or revision is inconsistent",
        ));
    }
    Ok(())
}
