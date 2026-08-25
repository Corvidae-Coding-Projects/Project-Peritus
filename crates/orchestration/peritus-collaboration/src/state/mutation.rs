//! Narrow mutation operations for the authoritative collaboration state.

use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};

use crate::{
    CollaborationMessageId, CollaborationPhase, CollaborationState, CollaborationTask,
    CollaborationTaskId, MessageDelivery, ReservationObservation, TaskPhase, TaskTerminal,
};

pub fn insert_task(state: &mut CollaborationState, task: CollaborationTask) {
    let id = task.assignment().task_id();
    let index = state
        .tasks
        .binary_search_by_key(&id, |record| record.assignment().task_id())
        .unwrap_or_else(core::convert::identity);
    state.tasks.insert(index, task);
}

pub fn task_mut(
    state: &mut CollaborationState,
    task_id: CollaborationTaskId,
) -> Option<&mut CollaborationTask> {
    state
        .tasks
        .binary_search_by_key(&task_id, |task| task.assignment().task_id())
        .ok()
        .map(|index| &mut state.tasks[index])
}

pub fn insert_message(state: &mut CollaborationState, delivery: MessageDelivery) {
    let id = delivery.message().id();
    let index = state
        .messages
        .binary_search_by_key(&id, |record| record.message().id())
        .unwrap_or_else(core::convert::identity);
    state.messages.insert(index, delivery);
}

pub fn message_mut(
    state: &mut CollaborationState,
    message_id: CollaborationMessageId,
) -> Option<&mut MessageDelivery> {
    state
        .messages
        .binary_search_by_key(&message_id, |delivery| delivery.message().id())
        .ok()
        .map(|index| &mut state.messages[index])
}

pub fn activate(
    state: &mut CollaborationState,
    task_id: CollaborationTaskId,
    observation: ReservationObservation,
) {
    if let Some(task) = task_mut(state, task_id) {
        task.set_reservation(observation);
        task.set_phase(TaskPhase::Active);
    }
}

pub fn terminate(
    state: &mut CollaborationState,
    task_id: CollaborationTaskId,
    terminal: TaskTerminal,
) {
    if let Some(task) = task_mut(state, task_id) {
        task.terminate(terminal);
    }
}

pub const fn set_phase(state: &mut CollaborationState, phase: CollaborationPhase) {
    state.phase = phase;
}

pub fn set_terminal(state: &mut CollaborationState, terminal: crate::CollaborationTerminal) {
    state.phase = CollaborationPhase::Terminal;
    state.terminal = Some(terminal);
}

pub fn advance_cursor(
    state: &mut CollaborationState,
    sequence: EventSequence,
    event_id: EventId,
    command_id: CommandId,
) {
    state.sequence = sequence;
    state.last_event_id = event_id;
    state.used_commands.push(command_id);
}

pub const fn set_state_digest(state: &mut CollaborationState, digest: Sha256Digest) {
    state.state_digest = digest;
}
