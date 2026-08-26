//! Focused canonical family and dispatcher tests.

use crate::{
    AppDiagnostic, AppErrorCode, AppEventEnvelope, AppEventPayload, AppProtocolError,
    AppProtocolLimits, AppRequestEnvelope, AppRequestPayload, AppResponseEnvelope,
    AppResponsePayload, ClientHello, ControlEnvelope, ControlPayload, CorrelationId, EventCursor,
    HeartbeatId, HeartbeatReply, ImplementationMetadata, IncompatibilityReason, NegotiationOutcome,
    ProtocolContext, ProtocolId, ProtocolVersion, RequestId, ServerHello, SubscriptionFilter,
    SubscriptionId, SubscriptionRequest, VersionRange,
};
use peritus_codec::{CodecLimits, decode_message, encode_frame, encode_message};
use peritus_types::SessionId;

use super::{AppMessage, decode_app_message, encode_app_message};

#[test]
fn all_six_families_round_trip_and_typed_hello_decodes() -> Result<(), Box<dyn std::error::Error>> {
    let limits = AppProtocolLimits::PRODUCTION;
    let protocol_id = ProtocolId::new([1; 16]).expect("nonzero protocol id");
    let context = ProtocolContext::new(
        protocol_id,
        ProtocolVersion::new(1, 0)?,
        SessionId::new([2; 16]).expect("nonzero session id"),
    );
    let request_id = RequestId::new([3; 16]).expect("nonzero request id");
    let correlation_id = CorrelationId::new([4; 16]).expect("nonzero correlation id");

    let client = ClientHello::new(
        protocol_id,
        vec![VersionRange::new(1, 0, 0)?],
        Vec::new(),
        Vec::new(),
        limits,
        "wire-test-client".to_owned(),
    )?;
    let typed = encode_message(&client, limits.codec())?;
    assert_eq!(decode_message::<ClientHello>(&typed, limits.codec())?, client);

    let server = ServerHello::new(
        protocol_id,
        ImplementationMetadata::new("wire-test-server".to_owned(), limits)?,
        NegotiationOutcome::Incompatible(IncompatibilityReason::NoCommonVersion),
    );
    let request = AppRequestEnvelope::new(
        context,
        request_id,
        correlation_id,
        AppRequestPayload::DaemonStatus,
    )?;
    let response = AppResponseEnvelope::new(
        context,
        request_id,
        correlation_id,
        AppResponsePayload::Error(AppProtocolError::new(AppErrorCode::NotReady, None)),
    );
    let event = AppEventEnvelope::new(
        context,
        AppEventPayload::Diagnostic(AppDiagnostic::new("bounded".to_owned(), 32)?),
    );
    let control = ControlEnvelope::new(
        context,
        correlation_id,
        ControlPayload::HeartbeatReply(HeartbeatReply::new(
            HeartbeatId::new([5; 16]).expect("nonzero heartbeat id"),
            7,
        )),
    );

    for message in [
        AppMessage::ClientHello(client),
        AppMessage::ServerHello(server),
        AppMessage::Request(request),
        AppMessage::Response(response),
        AppMessage::Event(event),
        AppMessage::Control(control),
    ] {
        let encoded = encode_app_message(&message, limits)?;
        assert_eq!(decode_app_message(&encoded, limits)?, message);
    }
    Ok(())
}

#[test]
fn dispatcher_separates_family_schema_tag_and_frame_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let limits = AppProtocolLimits::PRODUCTION;
    let unknown_family = encode_frame(120, 1, &[], limits.codec())?;
    assert_eq!(
        decode_app_message(&unknown_family, limits).map_err(|error| error.code()),
        Err(AppErrorCode::UnsupportedFamily),
    );

    let wrong_schema = encode_frame(crate::CLIENT_HELLO_FAMILY, 2, &[], limits.codec())?;
    assert_eq!(
        decode_app_message(&wrong_schema, limits).map_err(|error| error.code()),
        Err(AppErrorCode::UnsupportedSchema),
    );

    let request = minimal_status_request()?;
    let mut encoded = encode_app_message(&AppMessage::Request(request), limits)?;
    let final_byte = encoded.len() - 1;
    encoded[final_byte] = 250;
    assert_eq!(
        decode_app_message(&encoded, limits).map_err(|error| error.code()),
        Err(AppErrorCode::UnknownTag),
    );

    let mut truncated =
        encode_app_message(&AppMessage::Request(minimal_status_request()?), limits)?;
    truncated.pop();
    assert_eq!(
        decode_app_message(&truncated, limits).map_err(|error| error.code()),
        Err(AppErrorCode::TruncatedFrame),
    );

    let mut trailing = encode_app_message(&AppMessage::Request(minimal_status_request()?), limits)?;
    trailing.push(0);
    assert_eq!(
        decode_app_message(&trailing, limits).map_err(|error| error.code()),
        Err(AppErrorCode::TrailingBytes),
    );
    Ok(())
}

#[test]
fn negotiated_frame_limit_is_applied_before_payload_allocation()
-> Result<(), Box<dyn std::error::Error>> {
    let production = AppProtocolLimits::PRODUCTION;
    let encoded = encode_app_message(&AppMessage::Request(minimal_status_request()?), production)?;
    let constrained =
        AppProtocolLimits::new(CodecLimits::new(32, 16, 4, 8, 8, 4), 1, 1, 1, 1, 1, 8, 1, 8, 8, 1)?;
    assert_eq!(
        decode_app_message(&encoded, constrained).map_err(|error| error.code()),
        Err(AppErrorCode::LimitExceeded),
    );
    Ok(())
}

#[test]
fn encoder_rejects_value_built_above_negotiated_flow_limit()
-> Result<(), Box<dyn std::error::Error>> {
    let production = AppProtocolLimits::PRODUCTION;
    let context = ProtocolContext::new(
        ProtocolId::new([21; 16]).expect("nonzero protocol id"),
        ProtocolVersion::new(1, 0)?,
        SessionId::new([22; 16]).expect("nonzero session id"),
    );
    let request = AppRequestEnvelope::new(
        context,
        RequestId::new([23; 16]).expect("nonzero request id"),
        CorrelationId::new([24; 16]).expect("nonzero correlation id"),
        AppRequestPayload::Subscribe(SubscriptionRequest::new(
            SubscriptionId::new([25; 16]).expect("nonzero subscription id"),
            SubscriptionFilter::new(vec!["events".to_owned()], 2, 16)?,
            EventCursor::origin(),
            2,
            false,
        )?),
    )?;
    let constrained = AppProtocolLimits::new(
        production.codec(),
        production.max_versions(),
        production.max_features(),
        production.max_idempotency_entries(),
        production.max_topics(),
        1,
        production.max_artifact_chunk_bytes(),
        production.max_prompt_choices(),
        production.max_terminal_chunk_bytes(),
        production.max_diagnostic_bytes(),
        production.max_remaining_work_items(),
    )?;
    assert_eq!(
        encode_app_message(&AppMessage::Request(request), constrained)
            .map_err(|error| error.code()),
        Err(AppErrorCode::LimitExceeded),
    );
    Ok(())
}

fn minimal_status_request() -> Result<AppRequestEnvelope, AppProtocolError> {
    let context = ProtocolContext::new(
        ProtocolId::new([11; 16])
            .map_err(|_| AppProtocolError::new(AppErrorCode::InvalidIdentifier, None))?,
        ProtocolVersion::new(1, 0)?,
        SessionId::new([12; 16])
            .map_err(|_| AppProtocolError::new(AppErrorCode::InvalidIdentifier, None))?,
    );
    AppRequestEnvelope::new(
        context,
        RequestId::new([13; 16])
            .map_err(|_| AppProtocolError::new(AppErrorCode::InvalidIdentifier, None))?,
        CorrelationId::new([14; 16])
            .map_err(|_| AppProtocolError::new(AppErrorCode::InvalidIdentifier, None))?,
        AppRequestPayload::DaemonStatus,
    )
}
