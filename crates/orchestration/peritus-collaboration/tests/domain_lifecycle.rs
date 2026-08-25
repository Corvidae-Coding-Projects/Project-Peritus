//! Delegation, message, join, handoff, and terminal truth matrix.

#![allow(clippy::unwrap_used, reason = "fixed domain fixtures use checked values")]

mod support;

use peritus_collaboration::{
    ArtifactHandoff, CollaborationCommandKind, CollaborationMessage, CollaborationMessageId,
    CollaborationTaskId, CollaborationTerminalKind, JoinPolicy, ReservationObservation, TaskPhase,
    TaskTerminal, TaskTerminalKind, decide, join_is_truthful, replay, terminal_is_truthful,
};
use peritus_scheduler::DispatchId;
use peritus_types::{ActorId, ArtifactId};

use support::{Fixture, apply, bytes, command, digest};

#[allow(
    clippy::too_many_lines,
    reason = "the lifecycle test keeps one complete delegation-message-handoff-finalization trace visible"
)]
#[test]
fn all_required_lifecycle_message_handoff_and_finalization_are_exact() {
    let fixture = Fixture::new(JoinPolicy::AllRequired);
    let (state, mut events) = fixture.start();
    let mut state = fixture.activate_root(state, &mut events, 10);
    let child = fixture.child(30, 31, true, JoinPolicy::NoChildren);
    state = apply(
        state,
        &mut events,
        11,
        CollaborationCommandKind::OfferDelegation {
            offered_by: fixture.root_owner,
            assignment: child.clone(),
        },
    );
    assert_eq!(state.task(child.task_id()).unwrap().phase(), TaskPhase::Offered);
    state = apply(
        state,
        &mut events,
        12,
        CollaborationCommandKind::AcceptDelegation {
            task_id: child.task_id(),
            accepted_by: child.owner(),
        },
    );
    state = apply(
        state,
        &mut events,
        13,
        CollaborationCommandKind::ActivateTask {
            task_id: child.task_id(),
            observation: ReservationObservation::new(
                child.work_id(),
                DispatchId::new(bytes(50)).unwrap(),
                child.owner(),
                fixture.revision,
            ),
        },
    );
    let message = CollaborationMessage::new(
        CollaborationMessageId::new(bytes(51)).unwrap(),
        fixture.root_id,
        child.task_id(),
        child.owner(),
        fixture.root_owner,
        1,
        None,
        "application/vnd.peritus.handoff",
        128,
        digest(52),
        None,
        fixture.revision,
    )
    .unwrap();
    state = apply(
        state,
        &mut events,
        14,
        CollaborationCommandKind::SendMessage { message: message.clone() },
    );
    assert!(
        decide(
            &state,
            &command(
                &state,
                15,
                CollaborationCommandKind::AcknowledgeMessage {
                    message_id: message.id(),
                    receiver: fixture.root_owner,
                },
            ),
        )
        .is_ok()
    );
    state = apply(
        state,
        &mut events,
        15,
        CollaborationCommandKind::AcknowledgeMessage {
            message_id: message.id(),
            receiver: fixture.root_owner,
        },
    );
    let handoff = ArtifactHandoff::new(
        ArtifactId::new(bytes(53)).unwrap(),
        digest(54),
        digest(55),
        fixture.revision,
    )
    .unwrap();
    state = apply(
        state,
        &mut events,
        16,
        CollaborationCommandKind::CompleteTask {
            task_id: child.task_id(),
            completed_by: child.owner(),
            terminal: TaskTerminal::new(TaskTerminalKind::Succeeded, Some(handoff), digest(0))
                .unwrap(),
        },
    );
    state = apply(
        state,
        &mut events,
        17,
        CollaborationCommandKind::CompleteTask {
            task_id: fixture.root_id,
            completed_by: fixture.root_owner,
            terminal: TaskTerminal::new(TaskTerminalKind::Succeeded, None, digest(0)).unwrap(),
        },
    );
    state = apply(state, &mut events, 18, CollaborationCommandKind::Finalize);
    assert_eq!(state.terminal().unwrap().kind(), CollaborationTerminalKind::Completed);
    assert!(join_is_truthful(&state));
    assert!(terminal_is_truthful(&state));
    assert_eq!(replay(&events).unwrap(), state);
}

#[test]
fn missing_required_child_wrong_owner_and_stale_reservation_are_rejected_without_transition() {
    let fixture = Fixture::new(JoinPolicy::AllRequired);
    let (state, mut events) = fixture.start();
    let mut state = fixture.activate_root(state, &mut events, 60);
    let child = fixture.child(61, 62, true, JoinPolicy::NoChildren);
    state = apply(
        state,
        &mut events,
        63,
        CollaborationCommandKind::OfferDelegation {
            offered_by: fixture.root_owner,
            assignment: child.clone(),
        },
    );
    let success = TaskTerminal::new(TaskTerminalKind::Succeeded, None, digest(0)).unwrap();
    assert!(
        decide(
            &state,
            &command(
                &state,
                64,
                CollaborationCommandKind::CompleteTask {
                    task_id: fixture.root_id,
                    completed_by: fixture.root_owner,
                    terminal: success,
                },
            ),
        )
        .is_err()
    );
    assert!(
        decide(
            &state,
            &command(
                &state,
                65,
                CollaborationCommandKind::AcceptDelegation {
                    task_id: child.task_id(),
                    accepted_by: ActorId::new(bytes(99)).unwrap(),
                },
            ),
        )
        .is_err()
    );
    state = apply(
        state,
        &mut events,
        66,
        CollaborationCommandKind::AcceptDelegation {
            task_id: child.task_id(),
            accepted_by: child.owner(),
        },
    );
    let wrong = ReservationObservation::new(
        child.work_id(),
        DispatchId::new(bytes(67)).unwrap(),
        fixture.root_owner,
        fixture.revision,
    );
    assert!(
        decide(
            &state,
            &command(
                &state,
                68,
                CollaborationCommandKind::ActivateTask {
                    task_id: child.task_id(),
                    observation: wrong
                },
            ),
        )
        .is_err()
    );
}

#[test]
fn any_required_needs_one_declared_success_and_optional_failure_does_not_manufacture_it() {
    let fixture = Fixture::new(JoinPolicy::AnyRequired);
    let (state, mut events) = fixture.start();
    let mut state = fixture.activate_root(state, &mut events, 70);
    let optional = fixture.child(71, 72, false, JoinPolicy::NoChildren);
    state = apply(
        state,
        &mut events,
        73,
        CollaborationCommandKind::OfferDelegation {
            offered_by: fixture.root_owner,
            assignment: optional,
        },
    );
    assert!(!state.join_satisfied(fixture.root_id));
    assert!(
        decide(
            &state,
            &command(
                &state,
                74,
                CollaborationCommandKind::CompleteTask {
                    task_id: fixture.root_id,
                    completed_by: fixture.root_owner,
                    terminal: TaskTerminal::new(TaskTerminalKind::Succeeded, None, digest(0))
                        .unwrap(),
                },
            ),
        )
        .is_err()
    );
}

#[test]
fn identities_limits_and_causal_message_shape_are_checked() {
    assert!(CollaborationTaskId::new([0; 16]).is_err());
    assert!(peritus_collaboration::CollaborationLimits::new(0, 1, 1, 1, 1, 1, 1, 1, 1).is_err());
    let fixture = Fixture::new(JoinPolicy::NoChildren);
    assert!(
        CollaborationMessage::new(
            CollaborationMessageId::new(bytes(80)).unwrap(),
            fixture.root_id,
            fixture.root_id,
            fixture.root_owner,
            fixture.root_owner,
            2,
            None,
            "text/plain",
            1,
            digest(81),
            None,
            fixture.revision,
        )
        .is_err()
    );
}
