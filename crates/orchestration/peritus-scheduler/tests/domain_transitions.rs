//! Admission, dependency, ownership, retry, cancellation, and terminal matrices.

#![allow(clippy::unwrap_used, reason = "fixed checked test corpus")]

mod support;

use peritus_scheduler::{
    DispatchId, FailureDisposition, RecoveryPolicy, SchedulerCommandKind, SchedulerDirective,
    SchedulerErrorKind, SchedulerPhase, SchedulerTerminalKind, WorkId, WorkPhase, WorkTerminal,
    WorkerPhase, decide, pending_directives,
};

use support::{Fixture, bytes, digest};

#[test]
fn dependency_failure_propagates_and_never_manufactures_readiness() {
    let fixture = Fixture::new();
    let (mut state, mut events) = fixture.started();
    Fixture::apply(
        &mut state,
        &mut events,
        3,
        SchedulerCommandKind::RegisterWorker { descriptor: fixture.worker(30, 1) },
    );
    let prerequisite = fixture.work(40, 1, Vec::new(), None, 1, RecoveryPolicy::Fail);
    let dependent = fixture.work(41, 9, vec![prerequisite.id()], None, 1, RecoveryPolicy::Fail);
    Fixture::apply(
        &mut state,
        &mut events,
        4,
        SchedulerCommandKind::AdmitWork { spec: prerequisite },
    );
    Fixture::apply(&mut state, &mut events, 5, SchedulerCommandKind::AdmitWork { spec: dependent });
    assert_eq!(
        state.work_item(WorkId::new(bytes(41)).unwrap()).unwrap().phase(),
        WorkPhase::WaitingDependencies
    );
    let dispatch = DispatchId::new(bytes(50)).unwrap();
    Fixture::apply(
        &mut state,
        &mut events,
        6,
        SchedulerCommandKind::DispatchNext { dispatch_id: dispatch, dispatch_token: digest(50) },
    );
    Fixture::apply(
        &mut state,
        &mut events,
        7,
        SchedulerCommandKind::AcknowledgeStart { dispatch_id: dispatch },
    );
    Fixture::apply(
        &mut state,
        &mut events,
        8,
        SchedulerCommandKind::FailWork {
            dispatch_id: dispatch,
            failure_digest: digest(51),
            disposition: FailureDisposition::Failed,
        },
    );
    assert!(matches!(state.work_item(WorkId::new(bytes(41)).unwrap()).unwrap().terminal(),
        Some(WorkTerminal::DependencyFailed { dependency }) if *dependency == WorkId::new(bytes(40)).unwrap()));
    assert_eq!(state.reservations().len(), 0);
}

#[test]
fn retryable_failure_is_explicit_and_attempt_bound_exhausts() {
    let fixture = Fixture::new();
    let (mut state, mut events) = fixture.started();
    Fixture::apply(
        &mut state,
        &mut events,
        3,
        SchedulerCommandKind::RegisterWorker { descriptor: fixture.worker(30, 1) },
    );
    Fixture::apply(
        &mut state,
        &mut events,
        4,
        SchedulerCommandKind::AdmitWork {
            spec: fixture.work(40, 1, Vec::new(), None, 2, RecoveryPolicy::RetrySafe),
        },
    );
    let first = DispatchId::new(bytes(50)).unwrap();
    Fixture::apply(
        &mut state,
        &mut events,
        5,
        SchedulerCommandKind::DispatchNext { dispatch_id: first, dispatch_token: digest(50) },
    );
    Fixture::apply(
        &mut state,
        &mut events,
        6,
        SchedulerCommandKind::FailWork {
            dispatch_id: first,
            failure_digest: digest(60),
            disposition: FailureDisposition::Retryable,
        },
    );
    assert_eq!(state.work()[0].phase(), WorkPhase::RetryPending);
    Fixture::apply(
        &mut state,
        &mut events,
        7,
        SchedulerCommandKind::RetryWork { work_id: WorkId::new(bytes(40)).unwrap() },
    );
    let second = DispatchId::new(bytes(51)).unwrap();
    Fixture::apply(
        &mut state,
        &mut events,
        8,
        SchedulerCommandKind::DispatchNext { dispatch_id: second, dispatch_token: digest(51) },
    );
    Fixture::apply(
        &mut state,
        &mut events,
        9,
        SchedulerCommandKind::FailWork {
            dispatch_id: second,
            failure_digest: digest(61),
            disposition: FailureDisposition::Retryable,
        },
    );
    assert!(matches!(state.work()[0].terminal(), Some(WorkTerminal::Exhausted { .. })));
    assert_eq!(state.work()[0].attempts_started(), 2);
}

#[test]
fn worker_loss_classifies_retry_ambiguity_failure_and_cancellation() {
    for (policy, expected) in [
        (RecoveryPolicy::RetrySafe, WorkPhase::Queued),
        (RecoveryPolicy::Ambiguous, WorkPhase::Terminal),
        (RecoveryPolicy::Fail, WorkPhase::Terminal),
    ] {
        let fixture = Fixture::new();
        let (mut state, mut events) = fixture.started();
        Fixture::apply(
            &mut state,
            &mut events,
            3,
            SchedulerCommandKind::RegisterWorker { descriptor: fixture.worker(30, 1) },
        );
        Fixture::apply(
            &mut state,
            &mut events,
            4,
            SchedulerCommandKind::AdmitWork {
                spec: fixture.work(40, 1, Vec::new(), None, 2, policy),
            },
        );
        Fixture::apply(
            &mut state,
            &mut events,
            5,
            SchedulerCommandKind::DispatchNext {
                dispatch_id: DispatchId::new(bytes(50)).unwrap(),
                dispatch_token: digest(50),
            },
        );
        Fixture::apply(
            &mut state,
            &mut events,
            6,
            SchedulerCommandKind::LoseWorker {
                worker_id: peritus_scheduler::WorkerId::new(bytes(30)).unwrap(),
            },
        );
        assert_eq!(state.work()[0].phase(), expected);
        assert_eq!(state.workers()[0].phase(), WorkerPhase::Lost);
        assert!(state.reservations().is_empty());
    }
}

#[test]
fn cancellation_tree_preserves_active_ownership_until_acknowledgement() {
    let fixture = Fixture::new();
    let (mut state, mut events) = fixture.started();
    Fixture::apply(
        &mut state,
        &mut events,
        3,
        SchedulerCommandKind::RegisterWorker { descriptor: fixture.worker(30, 1) },
    );
    let root = fixture.work(40, 3, Vec::new(), None, 1, RecoveryPolicy::Fail);
    let root_id = root.id();
    Fixture::apply(&mut state, &mut events, 4, SchedulerCommandKind::AdmitWork { spec: root });
    Fixture::apply(
        &mut state,
        &mut events,
        5,
        SchedulerCommandKind::AdmitWork {
            spec: fixture.work(41, 2, Vec::new(), Some(root_id), 1, RecoveryPolicy::Fail),
        },
    );
    let dispatch = DispatchId::new(bytes(50)).unwrap();
    Fixture::apply(
        &mut state,
        &mut events,
        6,
        SchedulerCommandKind::DispatchNext { dispatch_id: dispatch, dispatch_token: digest(50) },
    );
    Fixture::apply(
        &mut state,
        &mut events,
        7,
        SchedulerCommandKind::CancelWorkTree { work_id: root_id },
    );
    assert_eq!(state.work_item(root_id).unwrap().phase(), WorkPhase::Cancelling);
    assert!(matches!(
        state.work_item(WorkId::new(bytes(41)).unwrap()).unwrap().terminal(),
        Some(WorkTerminal::Cancelled)
    ));
    assert!(matches!(pending_directives(&state).as_slice(),
        [SchedulerDirective::Cancel { dispatch_id: observed, .. }] if *observed == dispatch));
    Fixture::apply(
        &mut state,
        &mut events,
        8,
        SchedulerCommandKind::AcknowledgeCancellation { dispatch_id: dispatch },
    );
    assert!(state.reservations().is_empty());
    assert!(matches!(state.work_item(root_id).unwrap().terminal(), Some(WorkTerminal::Cancelled)));
}

#[test]
fn pause_drain_and_terminal_truth_are_closed_and_explicit() {
    let fixture = Fixture::new();
    let (mut state, mut events) = fixture.started();
    Fixture::apply(
        &mut state,
        &mut events,
        3,
        SchedulerCommandKind::RegisterWorker { descriptor: fixture.worker(30, 1) },
    );
    Fixture::apply(&mut state, &mut events, 4, SchedulerCommandKind::PauseScheduler);
    assert_eq!(state.phase(), SchedulerPhase::Paused);
    let error = decide(
        &state,
        &Fixture::command(
            &state,
            5,
            SchedulerCommandKind::DispatchNext {
                dispatch_id: DispatchId::new(bytes(50)).unwrap(),
                dispatch_token: digest(50),
            },
        ),
    )
    .unwrap_err();
    assert_eq!(error.kind(), SchedulerErrorKind::IllegalTransition);
    Fixture::apply(&mut state, &mut events, 6, SchedulerCommandKind::DrainScheduler);
    assert_eq!(state.phase(), SchedulerPhase::DrainingPaused);
    Fixture::apply(&mut state, &mut events, 7, SchedulerCommandKind::ResumeScheduler);
    assert_eq!(state.phase(), SchedulerPhase::Draining);
    let error = decide(
        &state,
        &Fixture::command(
            &state,
            8,
            SchedulerCommandKind::AdmitWork {
                spec: fixture.work(40, 1, Vec::new(), None, 1, RecoveryPolicy::Fail),
            },
        ),
    )
    .unwrap_err();
    assert_eq!(error.kind(), SchedulerErrorKind::IllegalTransition);
    Fixture::apply(&mut state, &mut events, 9, SchedulerCommandKind::FinalizeScheduler);
    assert_eq!(state.terminal().unwrap().kind(), SchedulerTerminalKind::Completed);
    assert_eq!(state.phase(), SchedulerPhase::Terminal);
}
