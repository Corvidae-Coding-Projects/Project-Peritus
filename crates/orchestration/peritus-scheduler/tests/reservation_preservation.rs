//! Multi-worker reservation capacity and release/replay regression traces.

mod support;

use peritus_scheduler::{
    DispatchId, FailureDisposition, RecoveryPolicy, ResourceKind, SchedulerCommandKind,
    SchedulerErrorKind, SchedulerState, WorkId, WorkerId, decide, replay, reservations_fit,
    unique_dispatch_ownership,
};

use support::{Fixture, bytes, digest};

#[derive(Clone, Copy)]
enum ReleasePath {
    Complete,
    Fail,
    Cancellation,
    Abandon,
    WorkerLoss,
}

#[test]
fn every_release_path_frees_only_its_exact_capacity_and_replays() {
    for path in [
        ReleasePath::Complete,
        ReleasePath::Fail,
        ReleasePath::Cancellation,
        ReleasePath::Abandon,
        ReleasePath::WorkerLoss,
    ] {
        exercise_release_path(path);
    }
}

#[test]
fn retry_releases_capacity_and_reserves_a_fresh_dispatch_identity() {
    let fixture = Fixture::new();
    let (mut state, mut events) = saturated_state(
        &fixture,
        RecoveryPolicy::RetrySafe,
        2,
        false,
        SchedulerErrorKind::NoFeasibleWork,
    );
    let released = DispatchId::new(bytes(60)).unwrap();
    Fixture::apply(
        &mut state,
        &mut events,
        30,
        SchedulerCommandKind::FailWork {
            dispatch_id: released,
            failure_digest: digest(90),
            disposition: FailureDisposition::Retryable,
        },
    );
    Fixture::apply(
        &mut state,
        &mut events,
        31,
        SchedulerCommandKind::RetryWork { work_id: WorkId::new(bytes(40)).unwrap() },
    );
    let fresh = DispatchId::new(bytes(75)).unwrap();
    Fixture::apply(
        &mut state,
        &mut events,
        32,
        SchedulerCommandKind::DispatchNext { dispatch_id: fresh, dispatch_token: digest(75) },
    );
    assert!(state.reservation(released).is_none());
    let retry = state.reservation(fresh).unwrap();
    assert_eq!(retry.work_id(), WorkId::new(bytes(40)).unwrap());
    assert_eq!(retry.attempt().get(), 2);
    assert_capacity_and_ownership(&state, 8, 8_192, 4);
    assert_eq!(replay(&events).unwrap(), state);
}

#[test]
fn dispatch_rejections_preserve_error_precedence_and_state() {
    let fixture = Fixture::with_active_reservations(4);
    let (mut state, mut events) = saturated_state(
        &fixture,
        RecoveryPolicy::Fail,
        1,
        false,
        SchedulerErrorKind::LimitExceeded,
    );
    let retained = DispatchId::new(bytes(60)).unwrap();
    let at_capacity = Fixture::command(
        &state,
        30,
        SchedulerCommandKind::DispatchNext { dispatch_id: retained, dispatch_token: digest(90) },
    );
    let before = state.clone();
    assert_eq!(decide(&state, &at_capacity).unwrap_err().kind(), SchedulerErrorKind::LimitExceeded);
    assert_eq!(state, before);

    Fixture::apply(
        &mut state,
        &mut events,
        31,
        SchedulerCommandKind::FailWork {
            dispatch_id: retained,
            failure_digest: digest(91),
            disposition: FailureDisposition::Failed,
        },
    );
    let duplicate = Fixture::command(
        &state,
        32,
        SchedulerCommandKind::DispatchNext { dispatch_id: retained, dispatch_token: digest(92) },
    );
    let before = state.clone();
    assert_eq!(
        decide(&state, &duplicate).unwrap_err().kind(),
        SchedulerErrorKind::IdentityConflict
    );
    assert_eq!(state, before);

    let fresh = Fixture::command(
        &state,
        33,
        SchedulerCommandKind::DispatchNext {
            dispatch_id: DispatchId::new(bytes(75)).unwrap(),
            dispatch_token: digest(93),
        },
    );
    assert_eq!(decide(&state, &fresh).unwrap_err().kind(), SchedulerErrorKind::NoFeasibleWork);
    assert_eq!(replay(&events).unwrap(), state);
}

#[test]
fn cancellation_acknowledgement_and_abandonment_reject_without_partial_mutation() {
    let fixture = Fixture::new();
    let (state, events) = saturated_state(
        &fixture,
        RecoveryPolicy::Fail,
        1,
        false,
        SchedulerErrorKind::NoFeasibleWork,
    );
    let live = DispatchId::new(bytes(60)).unwrap();
    let acknowledge = Fixture::command(
        &state,
        30,
        SchedulerCommandKind::AcknowledgeCancellation { dispatch_id: live },
    );
    let before = state.clone();
    assert_eq!(
        decide(&state, &acknowledge).unwrap_err().kind(),
        SchedulerErrorKind::IllegalTransition
    );
    assert_eq!(state, before);

    let abandon = Fixture::command(
        &state,
        31,
        SchedulerCommandKind::AbandonDispatch {
            dispatch_id: DispatchId::new(bytes(75)).unwrap(),
            cause_digest: digest(91),
        },
    );
    assert_eq!(decide(&state, &abandon).unwrap_err().kind(), SchedulerErrorKind::UnknownIdentity);
    assert_eq!(state, before);
    assert_eq!(replay(&events).unwrap(), state);
}

fn exercise_release_path(path: ReleasePath) {
    let fixture = Fixture::new();
    let (mut state, mut events) = saturated_state(
        &fixture,
        RecoveryPolicy::Fail,
        1,
        true,
        SchedulerErrorKind::NoFeasibleWork,
    );
    let released = DispatchId::new(bytes(60)).unwrap();
    let released_work = WorkId::new(bytes(40)).unwrap();
    let unrelated: Vec<_> = state
        .reservations()
        .iter()
        .filter(|reservation| reservation.dispatch_id() != released)
        .map(peritus_scheduler::SchedulerReservation::dispatch_id)
        .collect();
    release(&mut state, &mut events, path, released);
    assert!(state.reservation(released).is_none());
    let terminal = state.work_item(released_work).unwrap().terminal();
    match path {
        ReleasePath::Complete => {
            assert!(matches!(terminal, Some(peritus_scheduler::WorkTerminal::Succeeded { .. })));
        }
        ReleasePath::Fail | ReleasePath::WorkerLoss => {
            assert!(matches!(terminal, Some(peritus_scheduler::WorkTerminal::Failed { .. })));
        }
        ReleasePath::Cancellation => {
            assert!(matches!(terminal, Some(peritus_scheduler::WorkTerminal::Cancelled)));
        }
        ReleasePath::Abandon => {
            assert!(matches!(
                terminal,
                Some(peritus_scheduler::WorkTerminal::Abandoned { cause_digest })
                    if *cause_digest == digest(90)
            ));
        }
    }
    for dispatch in &unrelated {
        assert!(state.reservation(*dispatch).is_some());
    }
    assert_capacity_and_ownership(&state, 6, 6_144, 3);

    let replacement = DispatchId::new(bytes(75)).unwrap();
    Fixture::apply(
        &mut state,
        &mut events,
        34,
        SchedulerCommandKind::DispatchNext { dispatch_id: replacement, dispatch_token: digest(75) },
    );
    assert_capacity_and_ownership(&state, 8, 8_192, 4);

    let before_late = state.clone();
    let late = Fixture::command(
        &state,
        35,
        SchedulerCommandKind::CompleteWork { dispatch_id: released, result_digest: digest(99) },
    );
    assert_eq!(decide(&state, &late).unwrap_err().kind(), SchedulerErrorKind::UnknownIdentity);
    assert_eq!(state, before_late);
    assert_eq!(replay(&events).unwrap(), state);
}

fn saturated_state(
    fixture: &Fixture,
    first_recovery: RecoveryPolicy,
    first_attempts: u16,
    retain_waiting_work: bool,
    expected_blocked: SchedulerErrorKind,
) -> (SchedulerState, Vec<peritus_scheduler::SchedulerEvent>) {
    let (mut state, mut events) = fixture.started();
    for (command, id, concurrency, cpu, memory) in
        [(3, 30, 1, 2, 2_048), (4, 31, 3, 6, 6_144), (5, 32, 1, 2, 2_048)]
    {
        Fixture::apply(
            &mut state,
            &mut events,
            command,
            SchedulerCommandKind::RegisterWorker {
                descriptor: fixture.worker_with_resources(
                    id,
                    concurrency,
                    &[(ResourceKind::CPU, cpu), (ResourceKind::MEMORY_BYTES, memory)],
                ),
            },
        );
    }
    let work_end = if retain_waiting_work { 45 } else { 44 };
    for (offset, id) in (40_u8..work_end).enumerate() {
        let recovery = if id == 40 { first_recovery } else { RecoveryPolicy::Fail };
        let attempts = if id == 40 { first_attempts } else { 1 };
        Fixture::apply(
            &mut state,
            &mut events,
            6 + u8::try_from(offset).unwrap(),
            SchedulerCommandKind::AdmitWork {
                spec: fixture.work_with_resources(
                    id,
                    attempts,
                    recovery,
                    &[(ResourceKind::CPU, 2), (ResourceKind::MEMORY_BYTES, 2_048)],
                ),
            },
        );
    }
    for (offset, dispatch) in (60_u8..64).enumerate() {
        Fixture::apply(
            &mut state,
            &mut events,
            12 + u8::try_from(offset).unwrap(),
            SchedulerCommandKind::DispatchNext {
                dispatch_id: DispatchId::new(bytes(dispatch)).unwrap(),
                dispatch_token: digest(dispatch),
            },
        );
    }
    assert_capacity_and_ownership(&state, 8, 8_192, 4);
    let blocked = Fixture::command(
        &state,
        20,
        SchedulerCommandKind::DispatchNext {
            dispatch_id: DispatchId::new(bytes(74)).unwrap(),
            dispatch_token: digest(74),
        },
    );
    assert_eq!(decide(&state, &blocked).unwrap_err().kind(), expected_blocked);
    (state, events)
}

fn release(
    state: &mut SchedulerState,
    events: &mut Vec<peritus_scheduler::SchedulerEvent>,
    path: ReleasePath,
    dispatch: DispatchId,
) {
    match path {
        ReleasePath::Complete => {
            Fixture::apply(
                state,
                events,
                30,
                SchedulerCommandKind::AcknowledgeStart { dispatch_id: dispatch },
            );
            Fixture::apply(
                state,
                events,
                31,
                SchedulerCommandKind::CompleteWork {
                    dispatch_id: dispatch,
                    result_digest: digest(90),
                },
            );
        }
        ReleasePath::Fail => {
            Fixture::apply(
                state,
                events,
                30,
                SchedulerCommandKind::FailWork {
                    dispatch_id: dispatch,
                    failure_digest: digest(90),
                    disposition: FailureDisposition::Failed,
                },
            );
        }
        ReleasePath::Cancellation => {
            Fixture::apply(
                state,
                events,
                30,
                SchedulerCommandKind::CancelWork { work_id: WorkId::new(bytes(40)).unwrap() },
            );
            Fixture::apply(
                state,
                events,
                31,
                SchedulerCommandKind::AcknowledgeCancellation { dispatch_id: dispatch },
            );
        }
        ReleasePath::Abandon => {
            Fixture::apply(
                state,
                events,
                30,
                SchedulerCommandKind::AbandonDispatch {
                    dispatch_id: dispatch,
                    cause_digest: digest(90),
                },
            );
        }
        ReleasePath::WorkerLoss => {
            Fixture::apply(
                state,
                events,
                30,
                SchedulerCommandKind::LoseWorker { worker_id: WorkerId::new(bytes(30)).unwrap() },
            );
        }
    }
}

fn assert_capacity_and_ownership(state: &SchedulerState, cpu: u64, memory: u64, count: usize) {
    assert_eq!(state.reservations().len(), count);
    let used = state.used_resources().unwrap().unwrap();
    assert_eq!(used.quantity(ResourceKind::CPU), cpu);
    assert_eq!(used.quantity(ResourceKind::MEMORY_BYTES), memory);
    assert!(reservations_fit(state));
    assert!(unique_dispatch_ownership(state));
}
