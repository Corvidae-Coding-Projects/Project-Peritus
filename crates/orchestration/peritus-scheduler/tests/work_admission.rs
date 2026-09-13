//! Public admission priority, capability witnesses, and exact retained work.

mod support;

use peritus_codec::CodecLimits;
use peritus_scheduler::{
    RecoveryPolicy, ResourceKind, SchedulerCommandKind, SchedulerErrorKind, SchedulerEventKind,
    SchedulerState, WorkId, WorkPhase, WorkSpec, WorkerId, decide, decode_scheduler_state,
    encode_scheduler_state, replay,
};
use support::{Fixture, bytes};

fn rejected(state: &SchedulerState, spec: WorkSpec, kind: SchedulerErrorKind, detail: &str) {
    let before = state.clone();
    let command = Fixture::command(state, 90, SchedulerCommandKind::AdmitWork { spec });
    let error = decide(state, &command).expect_err("admission is rejected");
    assert_eq!(error.kind(), kind);
    assert_eq!(error.detail(), detail);
    assert_eq!(state, &before);
}

#[test]
fn admission_references_and_resources_are_checked_before_worker_eligibility() {
    let fixture = Fixture::new();
    let (state, _) = fixture.started();
    let dependency = WorkId::new(bytes(60)).expect("nonzero dependency");
    let parent = WorkId::new(bytes(70)).expect("nonzero parent");
    rejected(
        &state,
        fixture.work(40, 1, vec![dependency], Some(parent), 1, RecoveryPolicy::Fail),
        SchedulerErrorKind::UnknownIdentity,
        "work dependency is absent",
    );
    rejected(
        &state,
        fixture.work(40, 1, Vec::new(), Some(parent), 1, RecoveryPolicy::Fail),
        SchedulerErrorKind::UnknownIdentity,
        "work parent is absent",
    );
    rejected(
        &state,
        fixture.work_with_resources(40, 1, RecoveryPolicy::Fail, &[(ResourceKind::CPU, 9)]),
        SchedulerErrorKind::ResourceConflict,
        "work request exceeds global scheduler capacity",
    );
    rejected(
        &state,
        fixture.work(40, 1, Vec::new(), None, 1, RecoveryPolicy::Fail),
        SchedulerErrorKind::InvalidInput,
        "no registered owner worker supports the work execution class and request",
    );
}

#[test]
fn draining_and_duplicate_checks_keep_their_rejection_priority() {
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
            spec: fixture.work(40, 1, Vec::new(), None, 1, RecoveryPolicy::Fail),
        },
    );
    let duplicate_over_capacity =
        fixture.work_with_resources(40, 1, RecoveryPolicy::Fail, &[(ResourceKind::CPU, 9)]);
    rejected(
        &state,
        duplicate_over_capacity.clone(),
        SchedulerErrorKind::IdentityConflict,
        "work identity is retained",
    );
    Fixture::apply(&mut state, &mut events, 5, SchedulerCommandKind::DrainScheduler);
    rejected(
        &state,
        duplicate_over_capacity,
        SchedulerErrorKind::IllegalTransition,
        "draining scheduler rejects work admission",
    );
    assert_eq!(replay(&events).expect("accepted admission history replays"), state);
}

#[test]
fn unavailable_workers_remain_admission_witnesses_until_removed() {
    let worker_id = WorkerId::new(bytes(30)).expect("nonzero worker");
    for unavailable in [
        SchedulerCommandKind::DrainWorker { worker_id },
        SchedulerCommandKind::LoseWorker { worker_id },
    ] {
        let fixture = Fixture::new();
        let (mut state, mut events) = fixture.started();
        Fixture::apply(
            &mut state,
            &mut events,
            3,
            SchedulerCommandKind::RegisterWorker { descriptor: fixture.worker(30, 1) },
        );
        Fixture::apply(&mut state, &mut events, 4, unavailable);
        let spec = fixture.work(40, 1, Vec::new(), None, 1, RecoveryPolicy::Fail);
        let admitted = Fixture::apply(
            &mut state,
            &mut events,
            5,
            SchedulerCommandKind::AdmitWork { spec: spec.clone() },
        );
        assert_eq!(
            admitted.event().kind(),
            &SchedulerEventKind::WorkAdmitted { spec: spec.clone() }
        );
        let record = state.work_item(spec.id()).expect("admitted work retained");
        assert_eq!(record.spec(), &spec);
        assert_eq!(record.phase(), WorkPhase::Queued);
        assert_eq!(record.enqueue_ordinal(), 1);
        assert_eq!(record.attempts_started(), 0);
        assert_eq!(record.bypasses(), 0);
        assert_eq!(record.retry_cause(), None);
        assert_eq!(record.terminal(), None);
        Fixture::apply(
            &mut state,
            &mut events,
            6,
            SchedulerCommandKind::RemoveWorker { worker_id },
        );
        rejected(
            &state,
            fixture.work(41, 1, Vec::new(), None, 1, RecoveryPolicy::Fail),
            SchedulerErrorKind::InvalidInput,
            "no registered owner worker supports the work execution class and request",
        );
        assert_eq!(replay(&events).expect("worker/admission history replays"), state);
        let encoded = encode_scheduler_state(&state, CodecLimits::PRODUCTION)
            .expect("admission checkpoint encodes");
        assert_eq!(
            decode_scheduler_state(&encoded, CodecLimits::PRODUCTION)
                .expect("admission checkpoint decodes"),
            state,
        );
    }
}
