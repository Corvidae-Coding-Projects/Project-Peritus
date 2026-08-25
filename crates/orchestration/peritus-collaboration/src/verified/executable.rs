//! Executable counterparts to the D3 collaboration specifications.

use crate::{
    CollaborationPhase, CollaborationState, CollaborationTerminalKind, CollaborationTransition,
    TaskPhase, TaskTerminalKind,
};

/// Returns whether every task has one acyclic, depth-consistent chain ending at root.
#[must_use]
pub fn causal_graph_is_valid(state: &CollaborationState) -> bool {
    let root = state.binding().root_task_id();
    state.tasks().iter().all(|task| {
        let assignment = task.assignment();
        if assignment.task_id() == root {
            assignment.parent_task_id().is_none() && assignment.depth() == 0
        } else {
            let mut cursor = Some(assignment.task_id());
            let mut steps = 0_u16;
            while let Some(id) = cursor {
                let Some(current) = state.task(id) else {
                    return false;
                };
                if id == root {
                    return steps == assignment.depth();
                }
                steps = match steps.checked_add(1) {
                    Some(value) => value,
                    None => return false,
                };
                if steps > state.limits().depth() {
                    return false;
                }
                cursor = current.assignment().parent_task_id();
            }
            false
        }
    })
}

/// Returns whether every successful task satisfies its declared child join.
#[must_use]
pub fn join_is_truthful(state: &CollaborationState) -> bool {
    state.tasks().iter().all(|task| {
        task.terminal().is_none_or(|terminal| {
            terminal.kind() != TaskTerminalKind::Succeeded
                || state.join_satisfied(task.assignment().task_id())
        })
    })
}

/// Returns whether cancellation-pending tasks and descendants cannot claim new success.
#[must_use]
pub fn cancellation_dominates(state: &CollaborationState) -> bool {
    state.tasks().iter().filter(|task| task.phase() == TaskPhase::Cancelling).all(|task| {
        task.terminal().is_none()
            && state
                .tasks()
                .iter()
                .filter(|candidate| {
                    state.is_descendant_of(
                        candidate.assignment().task_id(),
                        task.assignment().task_id(),
                    )
                })
                .all(|descendant| {
                    descendant.phase() == TaskPhase::Cancelling
                        || descendant
                            .terminal()
                            .is_some_and(|terminal| terminal.kind() != TaskTerminalKind::Succeeded)
                })
    })
}

/// Returns whether aggregate completion is supported by exact retained state.
#[must_use]
pub fn terminal_is_truthful(state: &CollaborationState) -> bool {
    state.terminal().is_none_or(|terminal| {
        if terminal.kind() != CollaborationTerminalKind::Completed {
            return state.phase() == CollaborationPhase::Terminal;
        }
        state.phase() == CollaborationPhase::Terminal
            && !state.has_pending_directives()
            && state.tasks().iter().all(|task| task.phase() == TaskPhase::Terminal)
            && state
                .task(state.binding().root_task_id())
                .and_then(crate::CollaborationTask::terminal)
                .is_some_and(|root| root.kind() == TaskTerminalKind::Succeeded)
            && join_is_truthful(state)
    })
}

/// Returns exact complete-state replay equivalence.
#[must_use]
pub fn replay_equivalent(expected: &CollaborationState, observed: &CollaborationState) -> bool {
    expected == observed && expected.state_digest() == observed.state_digest()
}

/// Returns whether a transition advances exactly once with exact fences.
#[must_use]
pub fn transition_is_legal(
    prior: &CollaborationState,
    transition: &CollaborationTransition,
) -> bool {
    let event = transition.event();
    let successor = transition.state();
    prior.phase() != CollaborationPhase::Terminal
        && event.run_id() == prior.run_id()
        && event.previous_event() == Some(prior.last_event_id())
        && event.prior_state_digest() == prior.state_digest()
        && event.sequence().get() == prior.sequence().get().saturating_add(1)
        && successor.sequence() == event.sequence()
        && successor.last_event_id() == event.id()
        && successor.state_digest() == event.successor_state_digest()
}
