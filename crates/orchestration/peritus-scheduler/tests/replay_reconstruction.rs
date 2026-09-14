//! Replay rejects changes to event outputs that are deliberately absent from reconstructed commands.

mod support;

use peritus_codec::CodecLimits;
use peritus_scheduler::{
    RecoveryPolicy, SchedulerCommandKind, SchedulerErrorKind, decode_scheduler_event,
    encode_scheduler_event, replay,
};

use support::{Fixture, bytes};

fn replace_unique(bytes: &mut [u8], from: &[u8], to: &[u8]) {
    assert_eq!(from.len(), to.len());
    let positions: Vec<_> = bytes
        .windows(from.len())
        .enumerate()
        .filter_map(|(index, candidate)| (candidate == from).then_some(index))
        .collect();
    assert_eq!(positions.len(), 1, "test field must have one canonical occurrence");
    let position = *positions.first().expect("one canonical occurrence was established");
    bytes
        .get_mut(position..position + to.len())
        .expect("canonical occurrence is inside the encoded frame")
        .copy_from_slice(to);
}

#[test]
fn replay_rejects_tampered_successor_digest_not_used_by_reconstruction() {
    let fixture = Fixture::new();
    let (mut state, mut events) = fixture.started();
    Fixture::apply(&mut state, &mut events, 3, SchedulerCommandKind::PauseScheduler);

    let accepted = events.last().expect("pause emits an event");
    let original = *accepted.successor_state_digest().as_bytes();
    let mut changed = original;
    changed[0] ^= 0xff;
    let mut encoded = encode_scheduler_event(accepted, CodecLimits::PRODUCTION)
        .expect("accepted pause event encodes");
    replace_unique(&mut encoded, &original, &changed);
    *events.get_mut(1).expect("pause follows genesis") =
        decode_scheduler_event(&encoded, CodecLimits::PRODUCTION)
            .expect("modified digest remains a valid inert event field");

    let error = replay(&events).expect_err("successor digest must match deterministic reduction");
    assert_eq!(error.kind(), SchedulerErrorKind::ReplayMismatch);
    assert_eq!(error.detail(), "scheduler event differs from deterministic reduction");
}

#[test]
fn replay_rejects_tampered_derived_cancellation_affected_set() {
    let fixture = Fixture::new();
    let (mut state, mut events) = fixture.started();
    Fixture::apply(
        &mut state,
        &mut events,
        3,
        SchedulerCommandKind::RegisterWorker { descriptor: fixture.worker(20, 4) },
    );
    let root = fixture.work(30, 1, Vec::new(), None, 1, RecoveryPolicy::Fail);
    let child = fixture.work(31, 1, Vec::new(), Some(root.id()), 1, RecoveryPolicy::Fail);
    Fixture::apply(&mut state, &mut events, 4, SchedulerCommandKind::AdmitWork { spec: root });
    Fixture::apply(&mut state, &mut events, 5, SchedulerCommandKind::AdmitWork { spec: child });
    Fixture::apply(
        &mut state,
        &mut events,
        6,
        SchedulerCommandKind::CancelWorkTree {
            work_id: peritus_scheduler::WorkId::new(bytes(30))
                .expect("fixed root identity is nonzero"),
        },
    );

    let accepted = events.last().expect("cancellation emits an event");
    let mut encoded = encode_scheduler_event(accepted, CodecLimits::PRODUCTION)
        .expect("accepted cancellation event encodes");
    replace_unique(&mut encoded, &bytes(31), &bytes(32));
    *events.last_mut().expect("cancellation remains the final event") =
        decode_scheduler_event(&encoded, CodecLimits::PRODUCTION)
            .expect("modified affected identity remains a valid inert event field");

    let error =
        replay(&events).expect_err("derived affected set must match deterministic reduction");
    assert_eq!(error.kind(), SchedulerErrorKind::ReplayMismatch);
    assert_eq!(error.detail(), "scheduler event differs from deterministic reduction");
}
