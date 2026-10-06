//! Application-protocol negotiation matrix integration tests.

use peritus_app_protocol::{
    AppErrorCode, AppProtocolLimits, ClientHello, IncompatibilityReason, NegotiationOutcome,
    ProtocolFeatureName, ProtocolId, ProtocolVersion, ServerCapabilities, VersionRange,
    WellKnownProtocolFeature, negotiate,
};
use peritus_types::SessionId;

fn protocol_id() -> ProtocolId {
    ProtocolId::new([1; 16]).expect("fixture protocol identity")
}

fn session_id() -> SessionId {
    SessionId::new([2; 16]).expect("fixture session identity")
}

fn feature(value: WellKnownProtocolFeature) -> ProtocolFeatureName {
    ProtocolFeatureName::well_known(value).expect("well-known feature is canonical")
}

#[test]
fn expanded_codec_contract_negotiates_with_legacy_peers_without_changing_wire_bytes() {
    use peritus_app_protocol::{AppMessage, decode_app_message, encode_app_message};
    use peritus_codec::CodecLimits;

    let production = AppProtocolLimits::PRODUCTION;
    let legacy = AppProtocolLimits::new(
        CodecLimits::LEGACY_V1,
        production.max_versions(),
        production.max_features(),
        production.max_idempotency_entries(),
        production.max_topics(),
        production.max_in_flight_events(),
        production.max_artifact_chunk_bytes(),
        production.max_prompt_choices(),
        production.max_terminal_chunk_bytes(),
        production.max_diagnostic_bytes(),
        production.max_remaining_work_items(),
    )
    .unwrap();
    for (client_limits, server_limits) in [(legacy, production), (production, legacy)] {
        let client = ClientHello::new(
            protocol_id(),
            vec![VersionRange::new(1, 0, 0).unwrap()],
            Vec::new(),
            Vec::new(),
            client_limits,
            "compatibility-client".to_owned(),
        )
        .unwrap();
        let message = AppMessage::ClientHello(client.clone());
        let bytes = encode_app_message(&message, legacy).unwrap();
        assert_eq!(encode_app_message(&message, production).unwrap(), bytes);
        assert_eq!(decode_app_message(&bytes, production).unwrap(), message);
        assert_eq!(decode_app_message(&bytes, legacy).unwrap(), message);
        let server = ServerCapabilities::new(
            vec![VersionRange::new(1, 0, 0).unwrap()],
            Vec::new(),
            server_limits,
            "compatibility-server".to_owned(),
        )
        .unwrap();
        let selected = negotiate(&client, &server, session_id()).unwrap();
        match selected.outcome() {
            NegotiationOutcome::Compatible(protocol) | NegotiationOutcome::Downgraded(protocol) => {
                assert_eq!(protocol.limits(), legacy);
            }
            NegotiationOutcome::Incompatible(reason) => {
                panic!("legacy peer must remain usable: {reason:?}")
            }
        }
        let response = AppMessage::ServerHello(selected);
        let bytes = encode_app_message(&response, legacy).unwrap();
        assert_eq!(decode_app_message(&bytes, production).unwrap(), response);
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "the single negotiation matrix keeps all version, feature, and limit outcomes comparable"
)]
#[test]
fn negotiation_matrix_is_deterministic_and_explicit() {
    let artifact = feature(WellKnownProtocolFeature::ArtifactTransfer);
    let shutdown = feature(WellKnownProtocolFeature::GracefulShutdown);
    let client = ClientHello::new(
        protocol_id(),
        vec![
            VersionRange::new(2, 0, 1).expect("valid range"),
            VersionRange::new(1, 0, 3).expect("valid range"),
        ],
        vec![artifact.clone()],
        vec![shutdown.clone()],
        AppProtocolLimits::PRODUCTION,
        "integration-client".to_owned(),
    )
    .expect("canonical client hello");
    let server = ServerCapabilities::new(
        vec![
            VersionRange::new(1, 0, 3).expect("valid range"),
            VersionRange::new(2, 0, 1).expect("valid range"),
        ],
        vec![shutdown.clone(), artifact.clone()],
        AppProtocolLimits::PRODUCTION,
        "integration-server".to_owned(),
    )
    .expect("canonical server capabilities");
    let reordered_server = ServerCapabilities::new(
        vec![
            VersionRange::new(2, 0, 1).expect("valid range"),
            VersionRange::new(1, 0, 3).expect("valid range"),
        ],
        vec![artifact.clone(), shutdown.clone()],
        AppProtocolLimits::PRODUCTION,
        "integration-server".to_owned(),
    )
    .expect("reordered capabilities canonicalize");

    let compatible = negotiate(&client, &server, session_id()).expect("negotiation succeeds");
    assert_eq!(
        compatible,
        negotiate(&client, &reordered_server, session_id()).expect("deterministic result")
    );
    assert_eq!(compatible.established_session(), Some(session_id()));
    match compatible.outcome() {
        NegotiationOutcome::Compatible(protocol) => {
            assert_eq!(protocol.version(), ProtocolVersion::new(2, 1).expect("valid version"));
            assert!(protocol.features().contains(&artifact));
            assert!(protocol.features().contains(&shutdown));
            assert_eq!(protocol.limits(), AppProtocolLimits::PRODUCTION);
        }
        other => panic!("expected compatible negotiation, got {other:?}"),
    }

    let downgraded_server = ServerCapabilities::new(
        vec![VersionRange::new(1, 0, 2).expect("valid range")],
        vec![artifact.clone()],
        AppProtocolLimits::PRODUCTION,
        "older-server".to_owned(),
    )
    .expect("older server capabilities");
    match negotiate(&client, &downgraded_server, session_id())
        .expect("downgrade is usable")
        .outcome()
    {
        NegotiationOutcome::Downgraded(protocol) => {
            assert_eq!(protocol.version(), ProtocolVersion::new(1, 2).expect("valid version"));
            assert!(protocol.features().contains(&artifact));
            assert!(!protocol.features().contains(&shutdown));
        }
        other => panic!("expected explicit downgrade, got {other:?}"),
    }

    let missing_feature_server = ServerCapabilities::new(
        vec![VersionRange::new(2, 0, 1).expect("valid range")],
        vec![shutdown],
        AppProtocolLimits::PRODUCTION,
        "missing-feature-server".to_owned(),
    )
    .expect("server capabilities");
    match negotiate(&client, &missing_feature_server, session_id())
        .expect("incompatibility is a successful negotiation observation")
        .outcome()
    {
        NegotiationOutcome::Incompatible(IncompatibilityReason::MissingRequiredFeatures(names)) => {
            assert_eq!(names.as_slice(), &[artifact]);
        }
        other => panic!("expected missing-required-feature result, got {other:?}"),
    }

    let other_major_server = ServerCapabilities::new(
        vec![VersionRange::new(3, 0, 1).expect("valid range")],
        Vec::new(),
        AppProtocolLimits::PRODUCTION,
        "other-major-server".to_owned(),
    )
    .expect("server capabilities");
    assert!(matches!(
        negotiate(&client, &other_major_server, session_id())
            .expect("incompatibility is explicit")
            .outcome(),
        NegotiationOutcome::Incompatible(IncompatibilityReason::NoCommonVersion)
    ));

    let overlap = ClientHello::new(
        protocol_id(),
        vec![
            VersionRange::new(1, 0, 2).expect("valid range"),
            VersionRange::new(1, 2, 3).expect("valid range"),
        ],
        Vec::new(),
        Vec::new(),
        AppProtocolLimits::PRODUCTION,
        "bad-client".to_owned(),
    )
    .expect_err("overlapping ranges are noncanonical");
    assert_eq!(overlap.code(), AppErrorCode::InvalidVersion);
}
