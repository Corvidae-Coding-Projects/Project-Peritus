//! Scheduler-specific dual-schema codec compatibility and rejection coverage.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use peritus_codec::{CanonicalEncode, CodecError, CodecErrorKind, CodecLimits, encode_message};
use peritus_scheduler::{
    SchedulerCommand, SchedulerCommandFrame, SchedulerCommandKind, SchedulerEventFrame,
    SchedulerSemantics, SchedulerStateFrame, decode_scheduler_command, decode_scheduler_event,
    decode_scheduler_state, encode_scheduler_command, encode_scheduler_event,
    encode_scheduler_state, start,
};
use peritus_types::{CommandId, EventId};

use support::Fixture;

const LIMITS: CodecLimits = CodecLimits::PRODUCTION;
type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn historical_v1_frames_decode_and_reencode_byte_exactly() -> TestResult {
    let command_bytes = fixture("scheduler-command.bin")?;
    let event_bytes = fixture("scheduler-event.bin")?;
    let state_bytes = fixture("scheduler-state.bin")?;

    let command = decode_scheduler_command(&command_bytes, LIMITS)?;
    let event = decode_scheduler_event(&event_bytes, LIMITS)?;
    let state = decode_scheduler_state(&state_bytes, LIMITS)?;

    assert_eq!(command.semantics(), SchedulerSemantics::LegacyQueueV1);
    assert_eq!(event.semantics(), SchedulerSemantics::LegacyQueueV1);
    assert_eq!(state.binding().semantics(), SchedulerSemantics::LegacyQueueV1);
    assert_eq!(encode_scheduler_command(&command, LIMITS)?, command_bytes);
    assert_eq!(encode_scheduler_event(&event, LIMITS)?, event_bytes);
    assert_eq!(encode_scheduler_state(&state, LIMITS)?, state_bytes);

    assert_eq!(
        encode_message(&SchedulerCommandFrame::from_command(&command), LIMITS)
            .map_err(CodecError::kind),
        Err(CodecErrorKind::WrongSchemaVersion)
    );
    assert_eq!(
        encode_message(&SchedulerEventFrame::new(event), LIMITS).map_err(CodecError::kind),
        Err(CodecErrorKind::WrongSchemaVersion)
    );
    assert_eq!(
        encode_message(&SchedulerStateFrame::from_state(&state), LIMITS).map_err(CodecError::kind),
        Err(CodecErrorKind::WrongSchemaVersion)
    );

    Ok(())
}

#[test]
fn current_frames_are_schema_v2_and_roundtrip_through_dual_helpers() -> TestResult {
    let fixture = Fixture::new();
    let command = SchedulerCommand::new(
        CommandId::new(support::bytes(1)).expect("fixed nonzero command identity"),
        EventId::new(support::bytes(2)).expect("fixed nonzero event identity"),
        fixture.binding.run_id(),
        0,
        None,
        support::digest(0),
        fixture.binding.revision(),
        SchedulerCommandKind::StartScheduler { binding: fixture.binding },
    )?;
    let transition = start(&command)?;
    let command_bytes = encode_scheduler_command(&command, LIMITS)?;
    let event_bytes = encode_scheduler_event(transition.event(), LIMITS)?;
    let state_bytes = encode_scheduler_state(transition.state(), LIMITS)?;

    assert_eq!(SchedulerCommandFrame::SCHEMA_VERSION, 2);
    assert_eq!(SchedulerEventFrame::SCHEMA_VERSION, 2);
    assert_eq!(SchedulerStateFrame::SCHEMA_VERSION, 2);
    for bytes in [&command_bytes, &event_bytes, &state_bytes] {
        assert_eq!(&bytes[8..10], &2_u16.to_be_bytes());
    }
    assert_eq!(decode_scheduler_command(&command_bytes, LIMITS)?, command);
    assert_eq!(decode_scheduler_event(&event_bytes, LIMITS)?, *transition.event());
    assert_eq!(decode_scheduler_state(&state_bytes, LIMITS)?, *transition.state());

    Ok(())
}

#[test]
fn dual_helpers_reject_invalid_headers_lengths_and_trailing_bytes() -> TestResult {
    let fixture = Fixture::new();
    let command = SchedulerCommand::new(
        CommandId::new(support::bytes(1)).expect("fixed nonzero command identity"),
        EventId::new(support::bytes(2)).expect("fixed nonzero event identity"),
        fixture.binding.run_id(),
        0,
        None,
        support::digest(0),
        fixture.binding.revision(),
        SchedulerCommandKind::StartScheduler { binding: fixture.binding },
    )?;
    let transition = start(&command)?;
    let cases: [(Vec<u8>, Decoder); 3] = [
        (encode_scheduler_command(&command, LIMITS)?, decode_command),
        (encode_scheduler_event(transition.event(), LIMITS)?, decode_event),
        (encode_scheduler_state(transition.state(), LIMITS)?, decode_state),
    ];

    for (bytes, decode) in cases {
        let mut wrong_family = bytes.clone();
        wrong_family[6..8].copy_from_slice(&999_u16.to_be_bytes());
        assert_error(decode, &wrong_family, CodecErrorKind::WrongFamily, 6);

        let mut zero_schema = bytes.clone();
        zero_schema[8..10].copy_from_slice(&0_u16.to_be_bytes());
        assert_error(decode, &zero_schema, CodecErrorKind::InvalidSchemaVersion, 8);

        let mut unsupported_schema = bytes.clone();
        unsupported_schema[8..10].copy_from_slice(&3_u16.to_be_bytes());
        assert_error(decode, &unsupported_schema, CodecErrorKind::WrongSchemaVersion, 8);

        let mut flags = bytes.clone();
        flags[10..12].copy_from_slice(&1_u16.to_be_bytes());
        assert_error(decode, &flags, CodecErrorKind::NonzeroFlags, 10);

        assert_error(decode, &bytes[..bytes.len() - 1], CodecErrorKind::Truncated, bytes.len() - 1);

        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_error(decode, &trailing, CodecErrorKind::TrailingBytes, bytes.len());
    }

    let event_bytes = encode_scheduler_event(transition.event(), LIMITS)?;
    assert_error(decode_command, &event_bytes, CodecErrorKind::WrongFamily, 6);

    Ok(())
}

type Decoder = fn(&[u8]) -> Result<(), CodecError>;

fn decode_command(input: &[u8]) -> Result<(), CodecError> {
    decode_scheduler_command(input, LIMITS)?;
    Ok(())
}

fn decode_event(input: &[u8]) -> Result<(), CodecError> {
    decode_scheduler_event(input, LIMITS)?;
    Ok(())
}

fn decode_state(input: &[u8]) -> Result<(), CodecError> {
    decode_scheduler_state(input, LIMITS)?;
    Ok(())
}

fn assert_error(decode: Decoder, input: &[u8], kind: CodecErrorKind, offset: usize) {
    assert!(matches!(
        decode(input),
        Err(error) if error.kind() == kind && error.offset() == offset
    ));
}

fn fixture(name: &str) -> Result<Vec<u8>, std::io::Error> {
    fs::read(fixture_root().join(name))
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..").join("fixtures/protocol/scheduler-v1")
}
