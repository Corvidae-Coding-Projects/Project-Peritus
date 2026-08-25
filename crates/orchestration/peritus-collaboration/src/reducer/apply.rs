//! Closed collaboration command application separated from fence and replay orchestration.

mod terminal;

use peritus_types::Sha256Digest;

use crate::error::{CollaborationError, CollaborationErrorKind, reject};
use crate::state::mutation;
use crate::{
    CollaborationCommandKind, CollaborationEventKind, CollaborationPhase, CollaborationState,
    CollaborationTask, CollaborationTaskId, MessageDelivery, TaskPhase, TaskTerminal,
    TaskTerminalKind,
};

use super::illegal;

pub(super) fn apply(
    state: &mut CollaborationState,
    command: &CollaborationCommandKind,
) -> Result<CollaborationEventKind, CollaborationError> {
    match command {
        CollaborationCommandKind::Start { .. } => Err(illegal("collaboration already started")),
        CollaborationCommandKind::OfferDelegation { offered_by, assignment } => {
            offer(state, *offered_by, assignment)
        }
        CollaborationCommandKind::AcceptDelegation { task_id, accepted_by } => {
            accept(state, *task_id, *accepted_by)
        }
        CollaborationCommandKind::RejectDelegation { task_id, rejected_by, reason_digest } => {
            reject_offer(state, *task_id, *rejected_by, *reason_digest)
        }
        CollaborationCommandKind::ActivateTask { task_id, observation } => {
            activate(state, *task_id, *observation)
        }
        CollaborationCommandKind::SendMessage { message } => send_message(state, message),
        CollaborationCommandKind::AcknowledgeMessage { message_id, receiver } => {
            acknowledge_message(state, *message_id, *receiver)
        }
        CollaborationCommandKind::CompleteTask { task_id, completed_by, terminal } => {
            terminal::complete(state, *task_id, *completed_by, *terminal)
        }
        CollaborationCommandKind::AbandonTask { task_id, abandoned_by, reason_digest } => {
            terminal::abandon(state, *task_id, *abandoned_by, *reason_digest)
        }
        CollaborationCommandKind::CancelTask { task_id, requested_by, reason_digest } => {
            terminal::cancel(state, *task_id, *requested_by, *reason_digest)
        }
        CollaborationCommandKind::AcknowledgeCancellation { task_id, owner } => {
            terminal::acknowledge_cancellation(state, *task_id, *owner)
        }
        CollaborationCommandKind::Pause { requested_by } => terminal::pause(state, *requested_by),
        CollaborationCommandKind::Resume { requested_by } => terminal::resume(state, *requested_by),
        CollaborationCommandKind::Finalize => terminal::finalize(state),
    }
}

fn offer(
    state: &mut CollaborationState,
    offered_by: peritus_types::ActorId,
    assignment: &crate::Delegation,
) -> Result<CollaborationEventKind, CollaborationError> {
    if state.phase() != CollaborationPhase::Active {
        return Err(illegal("new delegation is disabled while paused"));
    }
    if state.task(assignment.task_id()).is_some()
        || state.tasks().iter().any(|task| task.assignment().work_id() == assignment.work_id())
    {
        return Err(reject(
            CollaborationErrorKind::IdentityConflict,
            "delegation reuses a task or scheduler work identity",
        ));
    }
    if state.tasks().len() >= state.limits().tasks() as usize
        || assignment.root_task_id() != state.binding().root_task_id()
    {
        return Err(reject(
            CollaborationErrorKind::LimitExceeded,
            "delegation exceeds task bounds or reuses a task/work identity",
        ));
    }
    let parent_id = assignment.parent_task_id().ok_or_else(|| {
        reject(CollaborationErrorKind::CausalityViolation, "delegated child has no parent")
    })?;
    let parent = state.task(parent_id).ok_or_else(|| {
        reject(CollaborationErrorKind::UnknownIdentity, "delegation parent is unknown")
    })?;
    if parent.phase() != TaskPhase::Active
        || parent.assignment().owner() != offered_by
        || assignment.parent_owner() != offered_by
        || assignment.depth()
            != parent.assignment().depth().checked_add(1).ok_or_else(|| {
                reject(CollaborationErrorKind::LimitExceeded, "task depth overflowed")
            })?
        || assignment.depth() > state.limits().depth()
        || parent.assignment().join_policy() == crate::JoinPolicy::NoChildren
    {
        return Err(reject(
            CollaborationErrorKind::CausalityViolation,
            "delegation parent, owner, depth, lifecycle, or join declaration differs",
        ));
    }
    if state.children(parent_id).len() >= usize::from(state.limits().fan_out()) {
        return Err(reject(
            CollaborationErrorKind::LimitExceeded,
            "delegation exceeds the parent's fan-out bound",
        ));
    }
    mutation::insert_task(state, CollaborationTask::offered(assignment.clone()));
    Ok(CollaborationEventKind::DelegationOffered { offered_by, assignment: assignment.clone() })
}

fn accept(
    state: &mut CollaborationState,
    task_id: CollaborationTaskId,
    accepted_by: peritus_types::ActorId,
) -> Result<CollaborationEventKind, CollaborationError> {
    let task = owned_task(state, task_id, accepted_by)?;
    if task.phase() != TaskPhase::Offered {
        return Err(illegal("only an offered delegation may be accepted"));
    }
    mutation::task_mut(state, task_id).ok_or_else(unknown_task)?.set_phase(TaskPhase::Accepted);
    Ok(CollaborationEventKind::DelegationAccepted { task_id, accepted_by })
}

fn reject_offer(
    state: &mut CollaborationState,
    task_id: CollaborationTaskId,
    rejected_by: peritus_types::ActorId,
    reason_digest: Sha256Digest,
) -> Result<CollaborationEventKind, CollaborationError> {
    let task = owned_task(state, task_id, rejected_by)?;
    if task.phase() != TaskPhase::Offered || reason_digest == Sha256Digest::new([0; 32]) {
        return Err(illegal("delegation rejection requires an offered task and nonzero reason"));
    }
    let terminal = TaskTerminal::new(TaskTerminalKind::Rejected, None, reason_digest)?;
    mutation::terminate(state, task_id, terminal);
    Ok(CollaborationEventKind::DelegationRejected { task_id, rejected_by, reason_digest })
}

fn activate(
    state: &mut CollaborationState,
    task_id: CollaborationTaskId,
    observation: crate::ReservationObservation,
) -> Result<CollaborationEventKind, CollaborationError> {
    let task = state.task(task_id).ok_or_else(unknown_task)?;
    if task.phase() != TaskPhase::Accepted
        || observation.work_id() != task.assignment().work_id()
        || observation.owner() != task.assignment().owner()
        || observation.revision() != state.binding().revision()
    {
        return Err(reject(
            CollaborationErrorKind::BindingMismatch,
            "task activation differs from assignment or scheduler reservation",
        ));
    }
    mutation::activate(state, task_id, observation);
    Ok(CollaborationEventKind::TaskActivated { task_id, observation })
}

fn send_message(
    state: &mut CollaborationState,
    message: &crate::CollaborationMessage,
) -> Result<CollaborationEventKind, CollaborationError> {
    if state.messages().len() >= state.limits().messages() as usize
        || state.message(message.id()).is_some()
        || message.payload_bytes() > state.limits().payload_bytes()
        || message.root_task_id() != state.binding().root_task_id()
        || message.revision() != state.binding().revision()
    {
        return Err(reject(
            CollaborationErrorKind::LimitExceeded,
            "message exceeds limits or conflicts with aggregate binding/identity",
        ));
    }
    let task = state.task(message.task_id()).ok_or_else(unknown_task)?;
    if task.phase() != TaskPhase::Active {
        return Err(illegal("messages require an active task"));
    }
    let owner = task.assignment().owner();
    let parent_owner = task.assignment().parent_owner();
    if !((message.sender() == owner && message.receiver() == parent_owner)
        || (message.sender() == parent_owner && message.receiver() == owner))
    {
        return Err(reject(
            CollaborationErrorKind::OwnerMismatch,
            "message sender and receiver are not the task owner/parent-owner pair",
        ));
    }
    if message.artifact().is_some_and(|artifact| artifact.revision() != state.binding().revision())
    {
        return Err(reject(
            CollaborationErrorKind::BindingMismatch,
            "message artifact handoff has another revision",
        ));
    }
    let task_messages: Vec<_> = state
        .messages()
        .iter()
        .filter(|delivery| delivery.message().task_id() == message.task_id())
        .collect();
    let artifact_references =
        task_messages.iter().filter(|delivery| delivery.message().artifact().is_some()).count();
    if message.artifact().is_some()
        && artifact_references >= usize::from(state.limits().artifact_references())
    {
        return Err(reject(
            CollaborationErrorKind::LimitExceeded,
            "task artifact-reference bound is exhausted",
        ));
    }
    let expected =
        u32::try_from(task_messages.len()).ok().and_then(|value| value.checked_add(1)).ok_or_else(
            || reject(CollaborationErrorKind::LimitExceeded, "message ordinal overflowed"),
        )?;
    let predecessor = task_messages.last().map(|delivery| delivery.message().id());
    if message.ordinal() != expected || message.predecessor() != predecessor {
        return Err(reject(
            CollaborationErrorKind::CausalityViolation,
            "message ordinal or predecessor is not the contiguous per-task successor",
        ));
    }
    let mut recipients: Vec<_> =
        task_messages.iter().map(|delivery| delivery.message().receiver()).collect();
    recipients.push(message.receiver());
    recipients.sort_unstable();
    recipients.dedup();
    if recipients.len() > usize::from(state.limits().recipients()) {
        return Err(reject(
            CollaborationErrorKind::LimitExceeded,
            "task recipient bound is exhausted",
        ));
    }
    mutation::insert_message(state, MessageDelivery::pending(message.clone()));
    Ok(CollaborationEventKind::MessageSent { message: message.clone() })
}

fn acknowledge_message(
    state: &mut CollaborationState,
    message_id: crate::CollaborationMessageId,
    receiver: peritus_types::ActorId,
) -> Result<CollaborationEventKind, CollaborationError> {
    let delivery = state.message(message_id).ok_or_else(|| {
        reject(CollaborationErrorKind::UnknownIdentity, "message identity is unknown")
    })?;
    if delivery.acknowledged() || delivery.message().receiver() != receiver {
        return Err(reject(
            CollaborationErrorKind::OwnerMismatch,
            "message is already acknowledged or receiver differs",
        ));
    }
    mutation::message_mut(state, message_id)
        .ok_or_else(|| reject(CollaborationErrorKind::UnknownIdentity, "message vanished"))?
        .acknowledge();
    Ok(CollaborationEventKind::MessageAcknowledged { message_id, receiver })
}

pub(super) fn owned_task(
    state: &CollaborationState,
    task_id: CollaborationTaskId,
    actor: peritus_types::ActorId,
) -> Result<&CollaborationTask, CollaborationError> {
    let task = state.task(task_id).ok_or_else(unknown_task)?;
    if task.assignment().owner() == actor {
        Ok(task)
    } else {
        Err(reject(CollaborationErrorKind::OwnerMismatch, "actor differs from retained task owner"))
    }
}

pub(super) fn unknown_task() -> CollaborationError {
    reject(CollaborationErrorKind::UnknownIdentity, "task identity is unknown")
}
