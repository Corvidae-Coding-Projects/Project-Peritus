//! Task completion, cancellation, pause, and aggregate terminal truth.

use peritus_types::{ActorId, Sha256Digest};

use crate::error::{CollaborationError, CollaborationErrorKind, reject};
use crate::state::mutation;
use crate::{
    CancellationEffect, CollaborationEventKind, CollaborationPhase, CollaborationState,
    CollaborationTaskId, CollaborationTerminal, CollaborationTerminalKind, TaskPhase, TaskTerminal,
    TaskTerminalKind,
};

use super::{illegal, owned_task, unknown_task};

pub(super) fn complete(
    state: &mut CollaborationState,
    task_id: CollaborationTaskId,
    completed_by: ActorId,
    terminal: TaskTerminal,
) -> Result<CollaborationEventKind, CollaborationError> {
    let task = owned_task(state, task_id, completed_by)?;
    if task.phase() != TaskPhase::Active
        || !matches!(terminal.kind(), TaskTerminalKind::Succeeded | TaskTerminalKind::Failed)
    {
        return Err(illegal("only active work may complete as success or failure"));
    }
    if terminal.handoff().is_some_and(|handoff| handoff.revision() != state.binding().revision()) {
        return Err(reject(
            CollaborationErrorKind::BindingMismatch,
            "task handoff revision differs from collaboration revision",
        ));
    }
    if terminal.handoff().is_some() {
        let artifact_references = state
            .messages()
            .iter()
            .filter(|delivery| {
                delivery.message().task_id() == task_id && delivery.message().artifact().is_some()
            })
            .count();
        if artifact_references >= usize::from(state.limits().artifact_references()) {
            return Err(reject(
                CollaborationErrorKind::LimitExceeded,
                "task artifact-reference bound is exhausted by message handoffs",
            ));
        }
    }
    if terminal.kind() == TaskTerminalKind::Succeeded && !state.join_satisfied(task_id) {
        return Err(reject(
            CollaborationErrorKind::JoinUnsatisfied,
            "task success is blocked by its declared required-child join",
        ));
    }
    mutation::terminate(state, task_id, terminal);
    Ok(CollaborationEventKind::TaskCompleted { task_id, completed_by, terminal })
}

pub(super) fn abandon(
    state: &mut CollaborationState,
    task_id: CollaborationTaskId,
    abandoned_by: ActorId,
    reason_digest: Sha256Digest,
) -> Result<CollaborationEventKind, CollaborationError> {
    let task = owned_task(state, task_id, abandoned_by)?;
    if !matches!(task.phase(), TaskPhase::Accepted | TaskPhase::Active)
        || reason_digest == Sha256Digest::new([0; 32])
    {
        return Err(illegal("abandonment requires accepted/active ownership and a reason"));
    }
    mutation::terminate(
        state,
        task_id,
        TaskTerminal::new(TaskTerminalKind::Abandoned, None, reason_digest)?,
    );
    Ok(CollaborationEventKind::TaskAbandoned { task_id, abandoned_by, reason_digest })
}

pub(super) fn cancel(
    state: &mut CollaborationState,
    task_id: CollaborationTaskId,
    requested_by: ActorId,
    reason_digest: Sha256Digest,
) -> Result<CollaborationEventKind, CollaborationError> {
    let target = state.task(task_id).ok_or_else(unknown_task)?;
    if target.phase() == TaskPhase::Terminal || reason_digest == Sha256Digest::new([0; 32]) {
        return Err(illegal("cancellation target is terminal or reason digest is zero"));
    }
    if !owner_may_cancel(state, task_id, requested_by) {
        return Err(reject(
            CollaborationErrorKind::OwnerMismatch,
            "cancellation requester is not the task owner or an ancestor owner",
        ));
    }
    let affected: Vec<_> = state
        .tasks()
        .iter()
        .filter(|task| {
            let id = task.assignment().task_id();
            (id == task_id || state.is_descendant_of(id, task_id))
                && task.phase() != TaskPhase::Terminal
        })
        .map(|task| task.assignment().task_id())
        .collect();
    let mut effects = Vec::with_capacity(affected.len());
    for id in affected {
        let phase = state.task(id).ok_or_else(unknown_task)?.phase();
        if matches!(phase, TaskPhase::Active | TaskPhase::Cancelling) {
            mutation::task_mut(state, id)
                .ok_or_else(unknown_task)?
                .set_phase(TaskPhase::Cancelling);
            effects.push(CancellationEffect::new(id, TaskPhase::Cancelling));
        } else {
            mutation::terminate(
                state,
                id,
                TaskTerminal::new(TaskTerminalKind::Cancelled, None, reason_digest)?,
            );
            effects.push(CancellationEffect::new(id, TaskPhase::Terminal));
        }
    }
    Ok(CollaborationEventKind::CancellationPropagated {
        task_id,
        requested_by,
        reason_digest,
        effects,
    })
}

pub(super) fn acknowledge_cancellation(
    state: &mut CollaborationState,
    task_id: CollaborationTaskId,
    owner: ActorId,
) -> Result<CollaborationEventKind, CollaborationError> {
    let task = owned_task(state, task_id, owner)?;
    if task.phase() != TaskPhase::Cancelling {
        return Err(illegal("only a cancelling task may acknowledge cancellation"));
    }
    mutation::terminate(
        state,
        task_id,
        TaskTerminal::new(TaskTerminalKind::Cancelled, None, Sha256Digest::new([0; 32]))?,
    );
    Ok(CollaborationEventKind::CancellationAcknowledged { task_id, owner })
}

pub(super) fn pause(
    state: &mut CollaborationState,
    requested_by: ActorId,
) -> Result<CollaborationEventKind, CollaborationError> {
    ensure_root_owner(state, requested_by)?;
    if state.phase() != CollaborationPhase::Active {
        return Err(illegal("only an active collaboration may pause"));
    }
    mutation::set_phase(state, CollaborationPhase::Paused);
    Ok(CollaborationEventKind::Paused { requested_by })
}

pub(super) fn resume(
    state: &mut CollaborationState,
    requested_by: ActorId,
) -> Result<CollaborationEventKind, CollaborationError> {
    ensure_root_owner(state, requested_by)?;
    if state.phase() != CollaborationPhase::Paused {
        return Err(illegal("only a paused collaboration may resume"));
    }
    mutation::set_phase(state, CollaborationPhase::Active);
    Ok(CollaborationEventKind::Resumed { requested_by })
}

pub(super) fn finalize(
    state: &mut CollaborationState,
) -> Result<CollaborationEventKind, CollaborationError> {
    if state.has_pending_directives()
        || state.tasks().iter().any(|task| task.phase() != TaskPhase::Terminal)
    {
        return Err(reject(
            CollaborationErrorKind::JoinUnsatisfied,
            "collaboration retains nonterminal task, delivery, or cancellation work",
        ));
    }
    let root = state.task(state.binding().root_task_id()).ok_or_else(unknown_task)?;
    let root_terminal = root.terminal().ok_or_else(|| {
        reject(CollaborationErrorKind::JoinUnsatisfied, "root has no terminal outcome")
    })?;
    let kind = match root_terminal.kind() {
        TaskTerminalKind::Succeeded if state.join_satisfied(root.assignment().task_id()) => {
            CollaborationTerminalKind::Completed
        }
        TaskTerminalKind::Succeeded => CollaborationTerminalKind::UnsatisfiedJoin,
        TaskTerminalKind::Failed | TaskTerminalKind::Rejected => CollaborationTerminalKind::Failed,
        TaskTerminalKind::Cancelled => CollaborationTerminalKind::Cancelled,
        TaskTerminalKind::Abandoned => CollaborationTerminalKind::Abandoned,
    };
    let blocking_tasks = if kind == CollaborationTerminalKind::Completed {
        Vec::new()
    } else {
        state
            .tasks()
            .iter()
            .filter(|task| {
                task.terminal()
                    .is_none_or(|terminal| terminal.kind() != TaskTerminalKind::Succeeded)
            })
            .map(|task| task.assignment().task_id())
            .collect()
    };
    mutation::set_terminal(state, CollaborationTerminal::new(kind, blocking_tasks));
    Ok(CollaborationEventKind::Finalized)
}

fn owner_may_cancel(
    state: &CollaborationState,
    task_id: CollaborationTaskId,
    actor: ActorId,
) -> bool {
    let mut cursor = Some(task_id);
    for _ in 0..=state.limits().depth() {
        let Some(id) = cursor else {
            return false;
        };
        let Some(task) = state.task(id) else {
            return false;
        };
        if task.assignment().owner() == actor {
            return true;
        }
        cursor = task.assignment().parent_task_id();
    }
    false
}

fn ensure_root_owner(state: &CollaborationState, actor: ActorId) -> Result<(), CollaborationError> {
    let root = state.task(state.binding().root_task_id()).ok_or_else(unknown_task)?;
    if root.assignment().owner() == actor {
        Ok(())
    } else {
        Err(reject(
            CollaborationErrorKind::OwnerMismatch,
            "operation requires the retained root owner",
        ))
    }
}
