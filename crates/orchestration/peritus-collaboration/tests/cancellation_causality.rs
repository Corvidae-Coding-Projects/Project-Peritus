//! Canonical cancellation-tree propagation and pause semantics.

#![allow(clippy::unwrap_used, reason = "fixed cancellation fixtures use checked values")]

mod support;

use peritus_collaboration::{
    CollaborationCommandKind, CollaborationEventKind, CollaborationTaskId, Delegation, JoinPolicy,
    ReservationObservation, TaskPhase, TaskTerminal, TaskTerminalKind, cancellation_dominates,
    decide,
};
use peritus_role::HarnessRole;
use peritus_scheduler::{DispatchId, WorkId};
use peritus_types::ActorId;

use support::{Fixture, apply, bytes, command, digest};

#[allow(
    clippy::too_many_lines,
    reason = "the cancellation test keeps one complete root-child-grandchild causal trace visible"
)]
#[test]
fn cancellation_propagates_canonically_and_cannot_resurrect_success() {
    let fixture = Fixture::new(JoinPolicy::AllRequired);
    let (state, mut events) = fixture.start();
    let mut state = fixture.activate_root(state, &mut events, 10);
    let child = fixture.child(30, 31, true, JoinPolicy::AllRequired);
    state = apply(
        state,
        &mut events,
        11,
        CollaborationCommandKind::OfferDelegation {
            offered_by: fixture.root_owner,
            assignment: child.clone(),
        },
    );
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
                DispatchId::new(bytes(40)).unwrap(),
                child.owner(),
                fixture.revision,
            ),
        },
    );
    let grandchild = Delegation::child(
        CollaborationTaskId::new(bytes(32)).unwrap(),
        fixture.root_id,
        child.task_id(),
        2,
        ActorId::new(bytes(33)).unwrap(),
        HarnessRole::Fixer,
        child.owner(),
        WorkId::new(bytes(34)).unwrap(),
        digest(35),
        true,
        JoinPolicy::NoChildren,
    )
    .unwrap();
    state = apply(
        state,
        &mut events,
        14,
        CollaborationCommandKind::OfferDelegation {
            offered_by: child.owner(),
            assignment: grandchild.clone(),
        },
    );
    state = apply(
        state,
        &mut events,
        15,
        CollaborationCommandKind::CancelTask {
            task_id: fixture.root_id,
            requested_by: fixture.root_owner,
            reason_digest: digest(36),
        },
    );
    let CollaborationEventKind::CancellationPropagated { effects, .. } =
        events.last().unwrap().kind()
    else {
        panic!("expected cancellation fact");
    };
    assert!(effects.windows(2).all(|pair| pair[0].task_id() < pair[1].task_id()));
    assert_eq!(state.task(fixture.root_id).unwrap().phase(), TaskPhase::Cancelling);
    assert_eq!(state.task(child.task_id()).unwrap().phase(), TaskPhase::Cancelling);
    assert_eq!(state.task(grandchild.task_id()).unwrap().phase(), TaskPhase::Terminal);
    assert!(cancellation_dominates(&state));
    assert!(
        decide(
            &state,
            &command(
                &state,
                16,
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
    state = apply(
        state,
        &mut events,
        16,
        CollaborationCommandKind::AcknowledgeCancellation {
            task_id: child.task_id(),
            owner: child.owner(),
        },
    );
    state = apply(
        state,
        &mut events,
        17,
        CollaborationCommandKind::AcknowledgeCancellation {
            task_id: fixture.root_id,
            owner: fixture.root_owner,
        },
    );
    state = apply(state, &mut events, 18, CollaborationCommandKind::Finalize);
    assert_eq!(
        state.terminal().unwrap().kind(),
        peritus_collaboration::CollaborationTerminalKind::Cancelled
    );
    assert_eq!(peritus_collaboration::replay(&events).unwrap(), state);
}

#[test]
fn pause_blocks_only_new_delegation_and_resume_restores_it() {
    let fixture = Fixture::new(JoinPolicy::AllRequired);
    let (state, mut events) = fixture.start();
    let mut state = fixture.activate_root(state, &mut events, 50);
    state = apply(
        state,
        &mut events,
        51,
        CollaborationCommandKind::Pause { requested_by: fixture.root_owner },
    );
    assert_eq!(state.task(fixture.root_id).unwrap().phase(), TaskPhase::Active);
    let child = fixture.child(52, 53, true, JoinPolicy::NoChildren);
    assert!(
        decide(
            &state,
            &command(
                &state,
                54,
                CollaborationCommandKind::OfferDelegation {
                    offered_by: fixture.root_owner,
                    assignment: child.clone(),
                },
            ),
        )
        .is_err()
    );
    state = apply(
        state,
        &mut events,
        55,
        CollaborationCommandKind::Resume { requested_by: fixture.root_owner },
    );
    assert!(
        decide(
            &state,
            &command(
                &state,
                56,
                CollaborationCommandKind::OfferDelegation {
                    offered_by: fixture.root_owner,
                    assignment: child,
                },
            ),
        )
        .is_ok()
    );
}
