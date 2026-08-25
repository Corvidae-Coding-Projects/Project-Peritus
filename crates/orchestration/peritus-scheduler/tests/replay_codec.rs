//! Exact replay, codec roundtrip, and closed-frame rejection.

#![allow(clippy::unwrap_used, reason = "fixed checked test corpus")]

mod support;

use peritus_codec::{CanonicalEncode, CodecLimits, decode_message, encode_message};
use peritus_scheduler::{
    SchedulerCommandFrame, SchedulerCommandKind, SchedulerEventFrame, SchedulerStateFrame, replay,
};

use support::Fixture;

#[test]
fn genesis_frames_roundtrip_and_replay_exactly() {
    let fixture = Fixture::new();
    let (state, events) = fixture.started();
    let command = peritus_scheduler::SchedulerCommand::new(
        peritus_types::CommandId::new(support::bytes(1)).unwrap(),
        peritus_types::EventId::new(support::bytes(2)).unwrap(),
        fixture.binding.run_id(),
        0,
        None,
        support::digest(0),
        fixture.binding.revision(),
        SchedulerCommandKind::StartScheduler { binding: fixture.binding },
    )
    .unwrap();
    let command_bytes =
        encode_message(&SchedulerCommandFrame::from_command(&command), CodecLimits::PRODUCTION)
            .unwrap();
    let event_bytes =
        encode_message(&SchedulerEventFrame::new(events[0].clone()), CodecLimits::PRODUCTION)
            .unwrap();
    let state_bytes =
        encode_message(&SchedulerStateFrame::from_state(&state), CodecLimits::PRODUCTION).unwrap();
    assert_eq!(SchedulerCommandFrame::FAMILY, 70);
    assert_eq!(SchedulerEventFrame::FAMILY, 71);
    assert_eq!(SchedulerStateFrame::FAMILY, 72);
    assert_eq!(
        decode_message::<SchedulerCommandFrame>(&command_bytes, CodecLimits::PRODUCTION)
            .unwrap()
            .into_command(),
        command
    );
    assert_eq!(
        decode_message::<SchedulerEventFrame>(&event_bytes, CodecLimits::PRODUCTION)
            .unwrap()
            .into_event(),
        events[0]
    );
    assert!(
        decode_message::<SchedulerStateFrame>(&state_bytes, CodecLimits::PRODUCTION)
            .unwrap()
            .matches_state(&state)
    );
    assert_eq!(replay(&events).unwrap(), state);

    for bytes in [&command_bytes, &event_bytes, &state_bytes] {
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(peritus_codec::decode_frame(&trailing, CodecLimits::PRODUCTION).is_err());
        let mut unknown_family = bytes.clone();
        unknown_family[6..8].copy_from_slice(&999_u16.to_be_bytes());
        assert!(
            decode_message::<SchedulerCommandFrame>(&unknown_family, CodecLimits::PRODUCTION)
                .is_err()
        );
    }
}

#[test]
fn reordered_or_tampered_stream_fails_closed() {
    let fixture = Fixture::new();
    let (_, mut events) = fixture.started();
    events.push(events[0].clone());
    assert!(replay(&events).is_err());
}
