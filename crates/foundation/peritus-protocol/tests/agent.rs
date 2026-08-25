//! D0 canonical record round-trip and adversarial compatibility checks.

use peritus_codec::{CanonicalEncode, CodecErrorKind, CodecLimits, decode_message, encode_message};
use peritus_policy::ActorRole;
use peritus_protocol::{
    AgentCommandDto, AgentCommandKindDto, AgentCountersDto, AgentEventDto, AgentEventKindDto,
    AgentPhaseDto, AgentResumablePhaseDto, AgentStateDto,
};
use peritus_types::{
    AcceptanceSpecId, ActorId, AttemptId, CommandId, EnvironmentId, EventId, EventSequence,
    Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber, RevisionTuple, SessionId,
    Sha256Digest, TurnId, WorkspaceId,
};

fn revision() -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new([1; 16]).expect("id"),
        HarnessId::new([2; 16]).expect("id"),
        WorkspaceId::new([3; 16]).expect("id"),
        Generation::new(4).expect("generation"),
        RevisionNumber::new(5).expect("revision"),
        PolicyId::new([6; 16]).expect("id"),
        ProviderProfileId::new([7; 16]).expect("id"),
    )
}

const fn counters() -> AgentCountersDto {
    AgentCountersDto::new(1, 2, 3, 4, 5, 6, 7)
}

const fn phase() -> AgentPhaseDto {
    AgentPhaseDto::Active(AgentResumablePhaseDto::StreamingResponse)
}

fn command() -> AgentCommandDto {
    AgentCommandDto::new(
        CommandId::new([10; 16]).expect("id"),
        EventId::new([11; 16]).expect("id"),
        TurnId::new([12; 16]).expect("id"),
        1,
        Some(EventId::new([13; 16]).expect("id")),
        revision(),
        phase(),
        AgentCommandKindDto::ObserveProviderEvent,
        Sha256Digest::new([14; 32]),
        counters(),
        b"normalized-event".to_vec(),
        CodecLimits::PRODUCTION,
    )
    .expect("command")
}

fn event() -> AgentEventDto {
    AgentEventDto::new(
        EventId::new([11; 16]).expect("id"),
        CommandId::new([10; 16]).expect("id"),
        EventSequence::new(2).expect("sequence"),
        Some(EventId::new([13; 16]).expect("id")),
        TurnId::new([12; 16]).expect("id"),
        revision(),
        phase(),
        AgentEventKindDto::ProviderEventObserved,
        Sha256Digest::new([14; 32]),
        counters(),
        b"normalized-event".to_vec(),
        CodecLimits::PRODUCTION,
    )
    .expect("event")
}

fn state() -> AgentStateDto {
    AgentStateDto::new(
        TurnId::new([12; 16]).expect("id"),
        AttemptId::new([20; 16]).expect("id"),
        ActorId::new([21; 16]).expect("id"),
        ActorRole::Writer,
        SessionId::new([22; 16]).expect("id"),
        EnvironmentId::new([23; 16]).expect("id"),
        revision(),
        RevisionNumber::new(2).expect("revision"),
        RevisionNumber::new(3).expect("revision"),
        RevisionNumber::new(4).expect("revision"),
        EventSequence::new(2).expect("sequence"),
        EventId::new([11; 16]).expect("id"),
        phase(),
        counters(),
        Sha256Digest::new([14; 32]),
        b"checkpoint".to_vec(),
        CodecLimits::PRODUCTION,
    )
    .expect("state")
}

#[test]
fn all_agent_families_round_trip_exactly() {
    let limits = CodecLimits::PRODUCTION;
    let command = command();
    let event = event();
    let state = state();
    let command_bytes = encode_message(&command, limits).expect("encode command");
    let event_bytes = encode_message(&event, limits).expect("encode event");
    let state_bytes = encode_message(&state, limits).expect("encode state");
    assert_eq!(decode_message::<AgentCommandDto>(&command_bytes, limits).expect("decode"), command);
    assert_eq!(decode_message::<AgentEventDto>(&event_bytes, limits).expect("decode"), event);
    assert_eq!(decode_message::<AgentStateDto>(&state_bytes, limits).expect("decode"), state);
    assert_eq!(AgentCommandDto::FAMILY, 40);
    assert_eq!(AgentEventDto::FAMILY, 41);
    assert_eq!(AgentStateDto::FAMILY, 42);
}

#[test]
fn truncation_unknown_tags_digest_mismatch_and_trailing_bytes_are_rejected() {
    let limits = CodecLimits::PRODUCTION;
    let bytes = encode_message(&event(), limits).expect("event");
    assert_eq!(
        decode_message::<AgentEventDto>(&bytes[..bytes.len() - 1], limits)
            .expect_err("truncated")
            .kind(),
        CodecErrorKind::Truncated
    );

    let mut unknown = bytes.clone();
    let kind_offset = 16 + 16 + 16 + 8 + 1 + 16 + 16 + 96 + 4;
    unknown[kind_offset..kind_offset + 2].copy_from_slice(&u16::MAX.to_be_bytes());
    assert_eq!(
        decode_message::<AgentEventDto>(&unknown, limits).expect_err("unknown kind").kind(),
        CodecErrorKind::UnknownTag
    );

    let mut mismatched = bytes.clone();
    *mismatched.last_mut().expect("nonempty payload") ^= 1;
    assert_eq!(
        decode_message::<AgentEventDto>(&mismatched, limits).expect_err("digest mismatch").kind(),
        CodecErrorKind::InvalidDomainValue
    );

    let mut trailing = bytes;
    let payload_length = u32::from_be_bytes(trailing[12..16].try_into().expect("header"));
    trailing[12..16].copy_from_slice(&(payload_length + 1).to_be_bytes());
    trailing.push(0);
    assert_eq!(
        decode_message::<AgentEventDto>(&trailing, limits).expect_err("trailing").kind(),
        CodecErrorKind::TrailingBytes
    );
}

#[test]
fn configured_opaque_bound_is_enforced() {
    let limits = CodecLimits::new(1_024, 1_008, 8, 32, 4, 4);
    let error = AgentCommandDto::new(
        CommandId::new([1; 16]).expect("id"),
        EventId::new([2; 16]).expect("id"),
        TurnId::new([3; 16]).expect("id"),
        0,
        None,
        revision(),
        AgentPhaseDto::Active(AgentResumablePhaseDto::PreparingContext),
        AgentCommandKindDto::StartTurn,
        Sha256Digest::new([4; 32]),
        AgentCountersDto::default(),
        vec![0; 5],
        limits,
    )
    .expect_err("bound");
    assert_eq!(error.kind(), CodecErrorKind::LimitExceeded);
}

#[test]
fn sequence_and_predecessor_must_describe_the_same_causal_head() {
    let event_error = AgentEventDto::new(
        EventId::new([1; 16]).expect("id"),
        CommandId::new([2; 16]).expect("id"),
        EventSequence::new(2).expect("sequence"),
        None,
        TurnId::new([3; 16]).expect("id"),
        revision(),
        phase(),
        AgentEventKindDto::ProviderEventObserved,
        Sha256Digest::new([4; 32]),
        counters(),
        Vec::new(),
        CodecLimits::PRODUCTION,
    )
    .expect_err("non-genesis requires predecessor");
    assert_eq!(event_error.kind(), CodecErrorKind::InvalidDomainValue);

    let command_error = AgentCommandDto::new(
        CommandId::new([2; 16]).expect("id"),
        EventId::new([1; 16]).expect("id"),
        TurnId::new([3; 16]).expect("id"),
        1,
        None,
        revision(),
        phase(),
        AgentCommandKindDto::ObserveProviderEvent,
        Sha256Digest::new([4; 32]),
        counters(),
        Vec::new(),
        CodecLimits::PRODUCTION,
    )
    .expect_err("non-genesis command requires predecessor");
    assert_eq!(command_error.kind(), CodecErrorKind::InvalidDomainValue);
}

#[test]
fn debug_output_redacts_all_opaque_payload_bytes() {
    for rendered in [format!("{:?}", command()), format!("{:?}", event()), format!("{:?}", state())]
    {
        assert!(!rendered.contains("normalized-event"));
        assert!(!rendered.contains("checkpoint"));
        assert!(!rendered.contains("payload:"));
        assert!(rendered.contains("payload_len"));
        assert!(rendered.contains("payload_digest"));
    }
}
