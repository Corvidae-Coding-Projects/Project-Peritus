//! Mixed recovery policies preserve unrelated ownership and exact ordered loss events.

mod support;

use peritus_codec::{CodecLimits, decode_message, encode_message, sha256};
use peritus_scheduler::{
    DispatchId, LossOutcome, RecoveryPolicy, ResourceKind, SchedulerCommandKind,
    SchedulerErrorKind, SchedulerEvent, SchedulerEventKind, SchedulerState, SchedulerStateFrame,
    WorkId, WorkPhase, WorkTerminal, WorkerId, WorkerPhase, decide, replay,
};

use support::{Fixture, bytes, digest};

fn work_id(value: u8) -> WorkId {
    WorkId::new(bytes(value)).expect("fixed nonzero work identity")
}

fn dispatch_id(value: u8) -> DispatchId {
    DispatchId::new(bytes(value)).expect("fixed nonzero dispatch identity")
}

fn mixed_state(
    cancellation_policy: RecoveryPolicy,
    cancellation_attempts: u16,
    cancellation_started: bool,
) -> (SchedulerState, Vec<SchedulerEvent>, u8) {
    let fixture = Fixture::new();
    let (mut state, mut events) = fixture.started();
    Fixture::apply(
        &mut state,
        &mut events,
        3,
        SchedulerCommandKind::RegisterWorker { descriptor: fixture.worker(30, 5) },
    );
    Fixture::apply(
        &mut state,
        &mut events,
        4,
        SchedulerCommandKind::RegisterWorker { descriptor: fixture.worker(31, 1) },
    );
    let mut command_id = 5;
    for (work, dispatch, attempts, policy) in [
        (40, 90, 2, RecoveryPolicy::RetrySafe),
        (41, 60, 1, RecoveryPolicy::RetrySafe),
        (42, 80, 2, RecoveryPolicy::Ambiguous),
        (43, 50, 2, RecoveryPolicy::Fail),
        (44, 70, cancellation_attempts, cancellation_policy),
        (45, 65, 2, RecoveryPolicy::RetrySafe),
    ] {
        Fixture::apply(
            &mut state,
            &mut events,
            command_id,
            SchedulerCommandKind::AdmitWork {
                spec: fixture.work(work, 1, Vec::new(), None, attempts, policy),
            },
        );
        command_id += 1;
        Fixture::apply(
            &mut state,
            &mut events,
            command_id,
            SchedulerCommandKind::DispatchNext {
                dispatch_id: dispatch_id(dispatch),
                dispatch_token: digest(dispatch),
            },
        );
        command_id += 1;
        if (work == 44 && cancellation_started) || (work != 44 && work % 2 == 0) {
            Fixture::apply(
                &mut state,
                &mut events,
                command_id,
                SchedulerCommandKind::AcknowledgeStart { dispatch_id: dispatch_id(dispatch) },
            );
            command_id += 1;
        }
    }
    Fixture::apply(
        &mut state,
        &mut events,
        command_id,
        SchedulerCommandKind::CancelWork { work_id: work_id(44) },
    );
    command_id += 1;
    (state, events, command_id)
}

fn check_mixed_loss(
    cancellation_policy: RecoveryPolicy,
    cancellation_attempts: u16,
    cancellation_started: bool,
) {
    let (mut state, mut events, command_id) =
        mixed_state(cancellation_policy, cancellation_attempts, cancellation_started);
    let lost_worker = WorkerId::new(bytes(30)).expect("fixed worker identity");
    let other_worker = WorkerId::new(bytes(31)).expect("fixed worker identity");
    let before = state.clone();
    let other_reservation = before.reservation(dispatch_id(65)).expect("other reservation");
    assert_eq!(other_reservation.worker_id(), other_worker);
    assert_eq!(state.reservations().len(), 6);

    let transition = Fixture::apply(
        &mut state,
        &mut events,
        command_id,
        SchedulerCommandKind::LoseWorker { worker_id: lost_worker },
    );
    assert_eq!(
        transition.event().kind(),
        &SchedulerEventKind::WorkerLost {
            worker_id: lost_worker,
            outcomes: vec![
                LossOutcome::Failed { dispatch_id: dispatch_id(50), work_id: work_id(43) },
                LossOutcome::Exhausted { dispatch_id: dispatch_id(60), work_id: work_id(41) },
                LossOutcome::Cancelled { dispatch_id: dispatch_id(70), work_id: work_id(44) },
                LossOutcome::Ambiguous { dispatch_id: dispatch_id(80), work_id: work_id(42) },
                LossOutcome::Requeued { dispatch_id: dispatch_id(90), work_id: work_id(40) },
            ],
        }
    );
    assert_eq!(state.reservations(), std::slice::from_ref(other_reservation));
    assert_eq!(state.work_item(work_id(45)), before.work_item(work_id(45)));
    assert_eq!(state.worker(other_worker), before.worker(other_worker));
    assert_eq!(state.worker(lost_worker).expect("lost worker retained").phase(), WorkerPhase::Lost);
    assert_eq!(state.used_dispatches(), before.used_dispatches());
    let used = state.used_resources().expect("exact resources").expect("one active reservation");
    assert_eq!(used.quantity(ResourceKind::CPU), 1);
    assert_eq!(used.quantity(ResourceKind::MEMORY_BYTES), 256);
    for id in 40..=44 {
        let old = before.work_item(work_id(id)).expect("pre-loss work");
        let new = state.work_item(work_id(id)).expect("post-loss work");
        assert_eq!(new.spec(), old.spec());
        assert_eq!(new.attempts_started(), old.attempts_started());
        assert_eq!(new.enqueue_ordinal(), old.enqueue_ordinal());
    }
    let requeued = state.work_item(work_id(40)).expect("retryable work retained");
    assert_eq!(requeued.phase(), WorkPhase::Queued);
    assert!(requeued.terminal().is_none());
    assert_eq!(
        state.work_item(work_id(41)).expect("exhausted work").terminal(),
        Some(&WorkTerminal::Exhausted { cause_digest: sha256(dispatch_id(60).as_bytes()) })
    );
    assert_eq!(
        state.work_item(work_id(42)).expect("ambiguous work").terminal(),
        Some(&WorkTerminal::Ambiguous { dispatch_id: dispatch_id(80) })
    );
    assert_eq!(
        state.work_item(work_id(43)).expect("failed work").terminal(),
        Some(&WorkTerminal::Failed { failure_digest: sha256(dispatch_id(50).as_bytes()) })
    );
    assert_eq!(
        state.work_item(work_id(44)).expect("cancelled work").terminal(),
        Some(&WorkTerminal::Cancelled)
    );
    assert_eq!(replay(&events).expect("mixed loss replays"), state);
    let encoded = encode_message(&SchedulerStateFrame::from_state(&state), CodecLimits::PRODUCTION)
        .expect("mixed loss checkpoint encodes");
    assert!(
        decode_message::<SchedulerStateFrame>(&encoded, CodecLimits::PRODUCTION)
            .expect("mixed loss checkpoint validates")
            .matches_state(&state)
    );
}

#[test]
fn mixed_loss_reports_each_owned_dispatch_once_in_order_and_preserves_other_worker() {
    for policy in [RecoveryPolicy::RetrySafe, RecoveryPolicy::Ambiguous, RecoveryPolicy::Fail] {
        for attempts in [1, 2] {
            for started in [false, true] {
                check_mixed_loss(policy, attempts, started);
            }
        }
    }
}

#[test]
fn empty_worker_loss_has_exact_admission_errors_and_replays() {
    let fixture = Fixture::new();
    let (mut state, mut events) = fixture.started();
    let worker_id = WorkerId::new(bytes(30)).expect("fixed worker identity");
    let command = Fixture::command(&state, 3, SchedulerCommandKind::LoseWorker { worker_id });
    let error = decide(&state, &command).expect_err("missing worker is rejected");
    assert_eq!(error.kind(), SchedulerErrorKind::UnknownIdentity);
    assert_eq!(error.detail(), "worker is not registered");
    Fixture::apply(
        &mut state,
        &mut events,
        3,
        SchedulerCommandKind::RegisterWorker { descriptor: fixture.worker(30, 1) },
    );
    let lost =
        Fixture::apply(&mut state, &mut events, 4, SchedulerCommandKind::LoseWorker { worker_id });
    assert_eq!(
        lost.event().kind(),
        &SchedulerEventKind::WorkerLost { worker_id, outcomes: Vec::new() },
    );
    for phase in [WorkerPhase::Lost, WorkerPhase::Removed] {
        assert_eq!(state.worker(worker_id).expect("retained worker").phase(), phase);
        let command = Fixture::command(&state, 6, SchedulerCommandKind::LoseWorker { worker_id });
        let error = decide(&state, &command).expect_err("lost or removed worker is rejected");
        assert_eq!(error.kind(), SchedulerErrorKind::IllegalTransition);
        assert_eq!(error.detail(), "worker is already lost or removed");
        if phase == WorkerPhase::Lost {
            Fixture::apply(
                &mut state,
                &mut events,
                5,
                SchedulerCommandKind::RemoveWorker { worker_id },
            );
        }
    }
    assert_eq!(replay(&events).expect("empty loss and removal replay"), state);
}
