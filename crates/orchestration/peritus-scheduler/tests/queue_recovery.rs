//! Queue bounds remain valid when active work returns for another attempt.

mod support;

use peritus_codec::{CodecLimits, decode_message, encode_message};
use peritus_scheduler::{
    DispatchId, FailureDisposition, RecoveryPolicy, SchedulerBinding, SchedulerCommandKind,
    SchedulerErrorKind, SchedulerLimits, SchedulerStateFrame, WorkId, WorkPhase, WorkerId, decide,
    replay,
};

use support::{Fixture, bytes, digest};

fn fixture() -> Fixture {
    let original = Fixture::new();
    let limits = SchedulerLimits::new(1, 4, 1, 1, 8, 1, 4, 2, 1, 1_048_576, 4_194_304)
        .expect("valid independent queue and active limits");
    let binding = SchedulerBinding::new(
        original.binding.run_id(),
        original.binding.scheduler_id(),
        original.binding.revision(),
        limits,
        original.binding.capacity().clone(),
    )
    .expect("capacity fits the small scheduler");
    Fixture { limits, binding, owner: original.owner }
}

#[test]
fn full_queue_rejection_precedes_duplicate_identity() {
    let fixture = fixture();
    let (mut state, mut events) = fixture.started();
    Fixture::apply(
        &mut state,
        &mut events,
        3,
        SchedulerCommandKind::RegisterWorker { descriptor: fixture.worker(30, 1) },
    );
    let spec = fixture.work(40, 1, Vec::new(), None, 1, RecoveryPolicy::Fail);
    Fixture::apply(
        &mut state,
        &mut events,
        4,
        SchedulerCommandKind::AdmitWork { spec: spec.clone() },
    );
    let before = state.clone();
    let command = Fixture::command(&state, 5, SchedulerCommandKind::AdmitWork { spec });
    let error = decide(&state, &command).expect_err("queue bound is checked before identity");
    assert_eq!(error.kind(), SchedulerErrorKind::LimitExceeded);
    assert_eq!(error.detail(), "work retention or queue limit reached");
    assert_eq!(state, before);
}

fn recovery_roundtrip(worker_loss: bool, recovery_policy: RecoveryPolicy, started: bool) {
    let fixture = fixture();
    let (mut state, mut events) = fixture.started();
    let dispatch_id = DispatchId::new(bytes(60)).expect("fixed dispatch identity");
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
            spec: fixture.work(40, 1, Vec::new(), None, 2, recovery_policy),
        },
    );
    Fixture::apply(
        &mut state,
        &mut events,
        5,
        SchedulerCommandKind::DispatchNext { dispatch_id, dispatch_token: digest(60) },
    );
    if started {
        Fixture::apply(
            &mut state,
            &mut events,
            8,
            SchedulerCommandKind::AcknowledgeStart { dispatch_id },
        );
    }
    let before_admission = state.clone();
    let admission = Fixture::command(
        &state,
        6,
        SchedulerCommandKind::AdmitWork {
            spec: fixture.work(41, 1, Vec::new(), None, 1, RecoveryPolicy::Fail),
        },
    );
    assert_eq!(
        decide(&state, &admission).expect_err("recovery reserves the queue slot").kind(),
        SchedulerErrorKind::LimitExceeded
    );
    assert_eq!(state, before_admission, "rejected admission leaves exact state unchanged");
    let recovery = if worker_loss {
        SchedulerCommandKind::LoseWorker {
            worker_id: WorkerId::new(bytes(30)).expect("fixed worker identity"),
        }
    } else {
        SchedulerCommandKind::FailWork {
            dispatch_id,
            failure_digest: digest(70),
            disposition: FailureDisposition::Retryable,
        }
    };
    Fixture::apply(&mut state, &mut events, 7, recovery);
    assert_eq!(replay(&events).expect("accepted history replays"), state);
    let waiting = state
        .work()
        .iter()
        .filter(|record| {
            matches!(
                record.phase(),
                WorkPhase::Queued | WorkPhase::WaitingDependencies | WorkPhase::RetryPending,
            )
        })
        .count();
    assert!(
        waiting <= fixture.limits.queued_work() as usize,
        "recovery produced {waiting} queued records under limit {}",
        fixture.limits.queued_work()
    );
    let encoded = encode_message(&SchedulerStateFrame::from_state(&state), CodecLimits::PRODUCTION)
        .expect("accepted recovered state encodes");
    let decoded = decode_message::<SchedulerStateFrame>(&encoded, CodecLimits::PRODUCTION)
        .expect("accepted recovered state decodes within immutable queue bounds");
    assert!(decoded.matches_state(&state));
}

#[test]
fn worker_loss_cannot_overfill_the_queue_after_admission() {
    for policy in [RecoveryPolicy::RetrySafe, RecoveryPolicy::Ambiguous, RecoveryPolicy::Fail] {
        for started in [false, true] {
            recovery_roundtrip(true, policy, started);
        }
    }
}

#[test]
fn retryable_failure_cannot_overfill_the_queue_after_admission() {
    for policy in [RecoveryPolicy::RetrySafe, RecoveryPolicy::Ambiguous, RecoveryPolicy::Fail] {
        for started in [false, true] {
            recovery_roundtrip(false, policy, started);
        }
    }
}

#[test]
fn final_attempt_and_cancelling_work_release_recovery_pressure() {
    for cancel in [false, true] {
        let fixture = fixture();
        let (mut state, mut events) = fixture.started();
        let dispatch_id = DispatchId::new(bytes(60)).expect("fixed dispatch identity");
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
                spec: fixture.work(
                    40,
                    1,
                    Vec::new(),
                    None,
                    if cancel { 2 } else { 1 },
                    RecoveryPolicy::RetrySafe,
                ),
            },
        );
        Fixture::apply(
            &mut state,
            &mut events,
            5,
            SchedulerCommandKind::DispatchNext { dispatch_id, dispatch_token: digest(60) },
        );
        if cancel {
            Fixture::apply(
                &mut state,
                &mut events,
                8,
                SchedulerCommandKind::CancelWork {
                    work_id: WorkId::new(bytes(40)).expect("fixed work identity"),
                },
            );
            assert_eq!(state.work()[0].phase(), WorkPhase::Cancelling);
        }
        Fixture::apply(
            &mut state,
            &mut events,
            6,
            SchedulerCommandKind::AdmitWork {
                spec: fixture.work(41, 1, Vec::new(), None, 1, RecoveryPolicy::Fail),
            },
        );
        Fixture::apply(
            &mut state,
            &mut events,
            7,
            SchedulerCommandKind::LoseWorker {
                worker_id: WorkerId::new(bytes(30)).expect("fixed worker identity"),
            },
        );
        assert_eq!(state.work()[0].phase(), WorkPhase::Terminal);
        assert_eq!(state.work()[1].phase(), WorkPhase::Queued);
        assert_eq!(replay(&events).expect("boundary history replays"), state);
        let encoded =
            encode_message(&SchedulerStateFrame::from_state(&state), CodecLimits::PRODUCTION)
                .expect("boundary state encodes");
        assert!(
            decode_message::<SchedulerStateFrame>(&encoded, CodecLimits::PRODUCTION)
                .expect("boundary state decodes")
                .matches_state(&state)
        );
    }
}

#[test]
fn repeated_explicit_recovery_preserves_the_queue_until_the_exact_final_attempt() {
    for policy in [RecoveryPolicy::RetrySafe, RecoveryPolicy::Ambiguous, RecoveryPolicy::Fail] {
        let fixture = fixture();
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
                spec: fixture.work(40, 1, Vec::new(), None, 4, policy),
            },
        );
        for attempt in 0_u8..4 {
            let identity = 20 + attempt * 4;
            let dispatch_id =
                DispatchId::new(bytes(60 + attempt)).expect("fixed dispatch identity");
            Fixture::apply(
                &mut state,
                &mut events,
                identity,
                SchedulerCommandKind::DispatchNext {
                    dispatch_id,
                    dispatch_token: digest(60 + attempt),
                },
            );
            let admission = Fixture::command(
                &state,
                6,
                SchedulerCommandKind::AdmitWork {
                    spec: fixture.work(41, 1, Vec::new(), None, 1, RecoveryPolicy::Fail),
                },
            );
            if attempt < 3 {
                assert_eq!(
                    decide(&state, &admission).expect_err("future recovery owns queue slot").kind(),
                    SchedulerErrorKind::LimitExceeded
                );
            } else {
                let accepted = decide(&state, &admission).expect("final attempt cannot return");
                events.push(accepted.event().clone());
                state = accepted.into_state();
            }
            Fixture::apply(
                &mut state,
                &mut events,
                identity + 1,
                SchedulerCommandKind::FailWork {
                    dispatch_id,
                    failure_digest: digest(70),
                    disposition: FailureDisposition::Retryable,
                },
            );
            if attempt < 3 {
                assert_eq!(state.work()[0].phase(), WorkPhase::RetryPending);
                Fixture::apply(
                    &mut state,
                    &mut events,
                    identity + 2,
                    SchedulerCommandKind::RetryWork {
                        work_id: WorkId::new(bytes(40)).expect("fixed work identity"),
                    },
                );
            } else {
                assert_eq!(state.work()[0].phase(), WorkPhase::Terminal);
                assert_eq!(state.work()[1].phase(), WorkPhase::Queued);
            }
            assert_eq!(replay(&events).expect("repeated recovery history replays"), state);
            let encoded =
                encode_message(&SchedulerStateFrame::from_state(&state), CodecLimits::PRODUCTION)
                    .expect("repeated recovery state encodes");
            assert!(
                decode_message::<SchedulerStateFrame>(&encoded, CodecLimits::PRODUCTION)
                    .expect("every recovery checkpoint remains readable")
                    .matches_state(&state)
            );
        }
    }
}
