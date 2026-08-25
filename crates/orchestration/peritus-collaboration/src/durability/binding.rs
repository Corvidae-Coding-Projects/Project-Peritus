//! Cross-record semantic binding checks before C0 append planning.

use crate::{
    CollaborationCommand, CollaborationCommandKind, CollaborationError, CollaborationEventKind,
    CollaborationTransition,
};

pub(super) fn validate_binding(
    command: &CollaborationCommand,
    transition: &CollaborationTransition,
) -> Result<(), CollaborationError> {
    let event = transition.event();
    let state = transition.state();
    let mismatches = [
        command.event_id() != event.id(),
        command.command_id() != event.command_id(),
        command.run_id() != event.run_id(),
        command.run_id() != state.run_id(),
        command.expected_previous_event() != event.previous_event(),
        command.expected_sequence().checked_add(1) != Some(event.sequence().get()),
        command.revision() != event.revision(),
        state.binding().revision() != event.revision(),
        command.prior_state_digest() != event.prior_state_digest(),
        event.successor_state_digest() != state.state_digest(),
        event.sequence() != state.sequence(),
        event.id() != state.last_event_id(),
        !event_matches_command(command.kind(), event.kind()),
    ];
    if mismatches.into_iter().any(core::convert::identity) {
        Err(super::binding_error(
            "collaboration command, event, and checkpoint do not describe one transition",
        ))
    } else {
        Ok(())
    }
}

fn event_matches_command(
    command: &CollaborationCommandKind,
    event: &CollaborationEventKind,
) -> bool {
    match (command, event) {
        (
            CollaborationCommandKind::Start { binding: left },
            CollaborationEventKind::Started { binding: right },
        ) => left == right,
        (
            CollaborationCommandKind::OfferDelegation { offered_by: la, assignment: lb },
            CollaborationEventKind::DelegationOffered { offered_by: ra, assignment: rb },
        ) => la == ra && lb == rb,
        (
            CollaborationCommandKind::AcceptDelegation { task_id: la, accepted_by: lb },
            CollaborationEventKind::DelegationAccepted { task_id: ra, accepted_by: rb },
        )
        | (
            CollaborationCommandKind::AcknowledgeCancellation { task_id: la, owner: lb },
            CollaborationEventKind::CancellationAcknowledged { task_id: ra, owner: rb },
        ) => la == ra && lb == rb,
        (
            CollaborationCommandKind::RejectDelegation {
                task_id: la,
                rejected_by: lb,
                reason_digest: lc,
            },
            CollaborationEventKind::DelegationRejected {
                task_id: ra,
                rejected_by: rb,
                reason_digest: rc,
            },
        )
        | (
            CollaborationCommandKind::AbandonTask {
                task_id: la,
                abandoned_by: lb,
                reason_digest: lc,
            },
            CollaborationEventKind::TaskAbandoned {
                task_id: ra,
                abandoned_by: rb,
                reason_digest: rc,
            },
        )
        | (
            CollaborationCommandKind::CancelTask {
                task_id: la,
                requested_by: lb,
                reason_digest: lc,
            },
            CollaborationEventKind::CancellationPropagated {
                task_id: ra,
                requested_by: rb,
                reason_digest: rc,
                ..
            },
        ) => la == ra && lb == rb && lc == rc,
        (
            CollaborationCommandKind::ActivateTask { task_id: la, observation: lb },
            CollaborationEventKind::TaskActivated { task_id: ra, observation: rb },
        ) => la == ra && lb == rb,
        (
            CollaborationCommandKind::SendMessage { message: left },
            CollaborationEventKind::MessageSent { message: right },
        ) => left == right,
        (
            CollaborationCommandKind::AcknowledgeMessage { message_id: la, receiver: lb },
            CollaborationEventKind::MessageAcknowledged { message_id: ra, receiver: rb },
        ) => la == ra && lb == rb,
        (
            CollaborationCommandKind::CompleteTask { task_id: la, completed_by: lb, terminal: lc },
            CollaborationEventKind::TaskCompleted { task_id: ra, completed_by: rb, terminal: rc },
        ) => la == ra && lb == rb && lc == rc,
        (
            CollaborationCommandKind::Pause { requested_by: left },
            CollaborationEventKind::Paused { requested_by: right },
        )
        | (
            CollaborationCommandKind::Resume { requested_by: left },
            CollaborationEventKind::Resumed { requested_by: right },
        ) => left == right,
        (CollaborationCommandKind::Finalize, CollaborationEventKind::Finalized) => true,
        _ => false,
    }
}
