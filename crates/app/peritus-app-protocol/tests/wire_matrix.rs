//! Six-family canonical wire round-trip and rejection integration tests.

use peritus_app_protocol::{
    AppErrorCode, AppMessage, AppProtocolError, AppProtocolLimits, AppResponseEnvelope,
    AppResponsePayload, CorrelationId, ProtocolContext, ProtocolId, ProtocolVersion, RequestId,
    ResponsibleSubsystem, RetryDisposition, decode_app_message, encode_app_message,
    schema::generated_fixture_cases,
};
use peritus_types::SessionId;
use std::collections::BTreeSet;

#[test]
fn all_six_families_round_trip_and_reject_malformed_frames() {
    let fixtures = generated_fixture_cases().expect("canonical fixtures encode");
    let mut observed_families = BTreeSet::new();
    for fixture in &fixtures {
        let decoded = decode_app_message(&fixture.payload, AppProtocolLimits::PRODUCTION);
        if fixture.accepted {
            let message = decoded.expect("valid fixture decodes");
            observed_families.insert(message.family());
            assert_eq!(
                encode_app_message(&message, AppProtocolLimits::PRODUCTION)
                    .expect("decoded message re-encodes"),
                fixture.payload,
                "{} is not byte-canonical",
                fixture.case,
            );
        } else {
            assert_eq!(
                decoded.expect_err("invalid fixture rejects").code(),
                fixture.expected_error.expect("invalid fixture names stable error"),
                "{} returned the wrong stable error",
                fixture.case,
            );
        }
    }
    assert_eq!(observed_families, BTreeSet::from([94, 95, 96, 97, 98, 99]));

    let valid = fixtures
        .iter()
        .find(|fixture| fixture.case == "minimal-client-hello")
        .expect("minimal hello fixture");
    let mut truncated = valid.payload.clone();
    truncated.pop();
    assert_eq!(
        decode_app_message(&truncated, AppProtocolLimits::PRODUCTION)
            .expect_err("truncated payload rejects")
            .code(),
        AppErrorCode::TruncatedFrame,
    );

    let mut trailing = valid.payload.clone();
    trailing.push(0);
    assert_eq!(
        decode_app_message(&trailing, AppProtocolLimits::PRODUCTION)
            .expect_err("trailing bytes reject")
            .code(),
        AppErrorCode::TrailingBytes,
    );
}

#[test]
fn provider_and_workspace_error_allocations_round_trip_through_public_response_wire() {
    assert_eq!(ResponsibleSubsystem::Provider.tag(), 10);
    assert_eq!(ResponsibleSubsystem::Workspace.tag(), 11);

    let context = ProtocolContext::new(
        ProtocolId::new([1; 16]).expect("protocol"),
        ProtocolVersion::new(1, 0).expect("version"),
        SessionId::new([2; 16]).expect("session"),
    );
    for (subsystem, request_byte, correlation_byte) in
        [(ResponsibleSubsystem::Provider, 3, 5), (ResponsibleSubsystem::Workspace, 4, 6)]
    {
        let response = AppMessage::Response(AppResponseEnvelope::new(
            context,
            RequestId::new([request_byte; 16]).expect("request"),
            CorrelationId::new([correlation_byte; 16]).expect("correlation"),
            AppResponsePayload::Error(AppProtocolError::classified(
                AppErrorCode::MissingRequiredFeature,
                RetryDisposition::NewRequest,
                subsystem,
                None,
            )),
        ));
        let encoded = encode_app_message(&response, AppProtocolLimits::PRODUCTION)
            .expect("public error response encodes");
        assert_eq!(
            decode_app_message(&encoded, AppProtocolLimits::PRODUCTION)
                .expect("public error response decodes"),
            response,
        );
    }
}
