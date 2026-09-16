//! Derived refresh behavior across multiple dependency rounds and reservation ownership changes.

mod support;

use peritus_scheduler::{
    DispatchId, RecoveryPolicy, SchedulerCommandKind, WorkId, WorkPhase, WorkTerminal, WorkerId,
    WorkerPhase, replay,
};

use support::{Fixture, bytes, digest};

fn work_id(value: u8) -> WorkId {
    WorkId::new(bytes(value)).expect("fixed work identity is nonzero")
}

fn worker_id(value: u8) -> WorkerId {
    WorkerId::new(bytes(value)).expect("fixed worker identity is nonzero")
}

#[test]
fn dependency_refresh_reaches_reverse_order_cascade_and_selects_first_failed_dependency() {
    let fixture = Fixture::new();
    let (mut state, mut events) = fixture.started();
    Fixture::apply(
        &mut state,
        &mut events,
        3,
        SchedulerCommandKind::RegisterWorker { descriptor: fixture.worker(20, 4) },
    );
    let definitions = [
        fixture.work(40, 1, Vec::new(), None, 1, RecoveryPolicy::Fail),
        fixture.work(45, 1, Vec::new(), Some(work_id(40)), 1, RecoveryPolicy::Fail),
        fixture.work(46, 1, Vec::new(), Some(work_id(40)), 1, RecoveryPolicy::Fail),
        fixture.work(10, 1, vec![work_id(45), work_id(46)], None, 1, RecoveryPolicy::Fail),
        fixture.work(9, 1, vec![work_id(10)], None, 1, RecoveryPolicy::Fail),
        fixture.work(48, 1, Vec::new(), None, 1, RecoveryPolicy::Fail),
    ];
    for (identity, spec) in (4..).zip(definitions) {
        Fixture::apply(&mut state, &mut events, identity, SchedulerCommandKind::AdmitWork { spec });
    }
    for id in [9, 10] {
        assert_eq!(
            state.work_item(work_id(id)).expect("dependent was admitted").phase(),
            WorkPhase::WaitingDependencies,
        );
    }
    let unrelated = state.work_item(work_id(48)).expect("unrelated work was admitted").clone();
    Fixture::apply(
        &mut state,
        &mut events,
        10,
        SchedulerCommandKind::CancelWorkTree { work_id: work_id(40) },
    );
    for (id, dependency) in [(10, 45), (9, 10)] {
        assert_eq!(
            state.work_item(work_id(id)).expect("dependent retained").terminal(),
            Some(&WorkTerminal::DependencyFailed { dependency: work_id(dependency) }),
        );
    }
    assert_eq!(state.work_item(work_id(48)), Some(&unrelated));
    assert_eq!(replay(&events).expect("multi-round dependency history replays"), state);
}

#[test]
fn worker_refresh_counts_cancelling_ownership_and_preserves_inactive_phases() {
    let fixture = Fixture::new();
    let (mut state, mut events) = fixture.started();
    for (identity, id) in (3..).zip(20..=23) {
        Fixture::apply(
            &mut state,
            &mut events,
            identity,
            SchedulerCommandKind::RegisterWorker { descriptor: fixture.worker(id, 2) },
        );
    }
    for (identity, kind) in [
        (7, SchedulerCommandKind::DrainWorker { worker_id: worker_id(21) }),
        (8, SchedulerCommandKind::LoseWorker { worker_id: worker_id(22) }),
        (9, SchedulerCommandKind::DrainWorker { worker_id: worker_id(23) }),
        (10, SchedulerCommandKind::RemoveWorker { worker_id: worker_id(23) }),
    ] {
        Fixture::apply(&mut state, &mut events, identity, kind);
    }
    let inactive = state.workers()[1..].to_vec();
    for (identity, id) in [(11, 40), (12, 41)] {
        Fixture::apply(
            &mut state,
            &mut events,
            identity,
            SchedulerCommandKind::AdmitWork {
                spec: fixture.work(id, 1, Vec::new(), None, 1, RecoveryPolicy::Fail),
            },
        );
    }
    for (identity, id, phase) in [(13, 60, WorkerPhase::Available), (14, 61, WorkerPhase::Busy)] {
        Fixture::apply(
            &mut state,
            &mut events,
            identity,
            SchedulerCommandKind::DispatchNext {
                dispatch_id: DispatchId::new(bytes(id)).expect("fixed dispatch identity"),
                dispatch_token: digest(id),
            },
        );
        assert_eq!(state.worker(worker_id(20)).expect("active worker retained").phase(), phase);
        assert_eq!(&state.workers()[1..], inactive.as_slice());
    }
    Fixture::apply(
        &mut state,
        &mut events,
        15,
        SchedulerCommandKind::CancelWork { work_id: work_id(40) },
    );
    assert_eq!(state.reservations().len(), 2);
    assert_eq!(
        state.worker(worker_id(20)).expect("active worker retained").phase(),
        WorkerPhase::Busy,
    );
    Fixture::apply(
        &mut state,
        &mut events,
        16,
        SchedulerCommandKind::AcknowledgeCancellation {
            dispatch_id: DispatchId::new(bytes(60)).expect("fixed dispatch identity"),
        },
    );
    assert_eq!(state.reservations().len(), 1);
    assert_eq!(
        state.worker(worker_id(20)).expect("active worker retained").phase(),
        WorkerPhase::Available,
    );
    assert_eq!(&state.workers()[1..], inactive.as_slice());
    assert_eq!(replay(&events).expect("worker refresh history replays"), state);
}
