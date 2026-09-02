//! Focused canonical family and dispatcher tests.

use crate::{
    AppDiagnostic, AppErrorCode, AppEventEnvelope, AppEventPayload, AppProtocolError,
    AppProtocolLimits, AppRequestEnvelope, AppRequestPayload, AppResponseEnvelope,
    AppResponsePayload, ClientHello, ControlEnvelope, ControlPayload, CorrelationId, EventCursor,
    HeartbeatId, HeartbeatReply, ImplementationMetadata, IncompatibilityReason, NegotiationOutcome,
    ProductConversationMessage, ProductConversationRole, ProductDeliverable,
    ProductProviderSelection, ProductRunContinuation, ProductRunControl, ProductRunControlAction,
    ProductRunConversation, ProductRunConversationQuery, ProductRunPhase, ProductRunRequest,
    ProductRunSnapshot, ProtocolContext, ProtocolId, ProtocolVersion, RequestId, ServerHello,
    SubscriptionFilter, SubscriptionId, SubscriptionRequest, VersionRange,
};
use peritus_codec::{CodecLimits, decode_message, encode_frame, encode_message};
use peritus_types::{ProviderProfileId, RunId, SessionId, WorkspaceId};

use super::{AppMessage, decode_app_message, encode_app_message};

#[test]
fn product_retry_is_limited_to_unsuccessful_terminal_runs() {
    assert!(ProductRunPhase::Failed.retryable());
    assert!(ProductRunPhase::Cancelled.retryable());
    assert!(ProductRunPhase::RecoveryRequired.retryable());
    assert!(!ProductRunPhase::Complete.retryable());
    assert!(!ProductRunPhase::Writing.retryable());
    assert!(ProductRunPhase::WaitingForUser.terminal());
}

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
        None,
        NegotiationOutcome::Incompatible(IncompatibilityReason::NoCommonVersion),
    )?;
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
fn product_run_requests_and_snapshots_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let context = ProtocolContext::new(
        ProtocolId::new([31; 16]).expect("nonzero protocol id"),
        ProtocolVersion::new(1, 0)?,
        SessionId::new([32; 16]).expect("nonzero session id"),
    );
    let providers = ProductProviderSelection::new(
        ProviderProfileId::new([33; 16]).expect("nonzero writer provider id"),
        ProviderProfileId::new([34; 16]).expect("nonzero reviewer provider id"),
        ProviderProfileId::new([35; 16]).expect("nonzero fixer provider id"),
    );
    let run_id = RunId::new([36; 16]).expect("nonzero run id");
    let workspace_id = WorkspaceId::new([37; 16]).expect("nonzero workspace id");
    let request = AppRequestEnvelope::new(
        context,
        RequestId::new([38; 16]).expect("nonzero request id"),
        CorrelationId::new([39; 16]).expect("nonzero correlation id"),
        AppRequestPayload::StartProductRun(ProductRunRequest::new(
            run_id,
            workspace_id,
            providers,
            "implement the feature".to_owned(),
        )?),
    )?;
    let encoded =
        encode_app_message(&AppMessage::Request(request.clone()), AppProtocolLimits::PRODUCTION)?;
    assert_eq!(
        decode_app_message(&encoded, AppProtocolLimits::PRODUCTION)?,
        AppMessage::Request(request)
    );

    let snapshot = ProductRunSnapshot::new(
        run_id,
        workspace_id,
        providers,
        ProductRunPhase::Reviewing,
        1,
        "implement the feature".to_owned(),
        "reviewing the diff".to_owned(),
        "diff --git".to_owned(),
        "tests passed".to_owned(),
        "no blocking findings".to_owned(),
        "implemented".to_owned(),
    )?
    .with_deliverable(
        ProductDeliverable::new(
            "/managed/worktree".to_owned(),
            vec!["game/src/main.rs".to_owned()],
            vec![
                "cargo test --manifest-path game/Cargo.toml --all-targets --all-features"
                    .to_owned(),
            ],
            "cargo run --manifest-path game/Cargo.toml".to_owned(),
        )?
        .mark_accepted()
        .mark_exported("/state/exports/run.patch".to_owned())?,
    );
    let response = AppResponseEnvelope::new(
        context,
        RequestId::new([40; 16]).expect("nonzero request id"),
        CorrelationId::new([41; 16]).expect("nonzero correlation id"),
        AppResponsePayload::ProductRuns(vec![snapshot]),
    );
    let encoded =
        encode_app_message(&AppMessage::Response(response.clone()), AppProtocolLimits::PRODUCTION)?;
    assert_eq!(
        decode_app_message(&encoded, AppProtocolLimits::PRODUCTION)?,
        AppMessage::Response(response)
    );
    Ok(())
}

#[test]
fn all_product_handoff_controls_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let context = ProtocolContext::new(
        ProtocolId::new([42; 16]).expect("protocol"),
        ProtocolVersion::new(1, 0)?,
        SessionId::new([43; 16]).expect("session"),
    );
    let run_id = RunId::new([44; 16]).expect("run");
    for (index, action) in [
        ProductRunControlAction::Accept,
        ProductRunControlAction::Commit,
        ProductRunControlAction::Export,
        ProductRunControlAction::Discard,
    ]
    .into_iter()
    .enumerate()
    {
        let request = AppRequestEnvelope::new(
            context,
            RequestId::new([u8::try_from(index + 45).expect("request byte"); 16])
                .expect("request ID"),
            CorrelationId::new([49; 16]).expect("correlation"),
            AppRequestPayload::ControlProductRun(ProductRunControl::new(run_id, action)),
        )?;
        let encoded = encode_app_message(
            &AppMessage::Request(request.clone()),
            AppProtocolLimits::PRODUCTION,
        )?;
        assert_eq!(
            decode_app_message(&encoded, AppProtocolLimits::PRODUCTION)?,
            AppMessage::Request(request)
        );
    }
    Ok(())
}

#[test]
fn product_run_followups_and_conversations_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let context = ProtocolContext::new(
        ProtocolId::new([51; 16]).expect("protocol"),
        ProtocolVersion::new(1, 0)?,
        SessionId::new([52; 16]).expect("session"),
    );
    let run_id = RunId::new([53; 16]).expect("run");
    for payload in [
        AppRequestPayload::ContinueProductRun(ProductRunContinuation::new(
            run_id,
            "keep the controls on the left".to_owned(),
        )?),
        AppRequestPayload::QueryProductRunConversation(ProductRunConversationQuery::new(run_id)),
    ] {
        let request = AppRequestEnvelope::new(
            context,
            RequestId::new([payload_tag(&payload); 16]).expect("request"),
            CorrelationId::new([55; 16]).expect("correlation"),
            payload,
        )?;
        let encoded = encode_app_message(
            &AppMessage::Request(request.clone()),
            AppProtocolLimits::PRODUCTION,
        )?;
        assert_eq!(
            decode_app_message(&encoded, AppProtocolLimits::PRODUCTION)?,
            AppMessage::Request(request)
        );
    }

    let conversation = ProductRunConversation::new(
        run_id,
        vec![
            ProductConversationMessage::new(
                ProductConversationRole::User,
                "build the game".to_owned(),
            )?,
            ProductConversationMessage::new(
                ProductConversationRole::Agent,
                "Which rendering library do you prefer?".to_owned(),
            )?,
        ],
    )?;
    let response = AppResponseEnvelope::new(
        context,
        RequestId::new([56; 16]).expect("request"),
        CorrelationId::new([57; 16]).expect("correlation"),
        AppResponsePayload::ProductRunConversation(conversation),
    );
    let encoded =
        encode_app_message(&AppMessage::Response(response.clone()), AppProtocolLimits::PRODUCTION)?;
    assert_eq!(
        decode_app_message(&encoded, AppProtocolLimits::PRODUCTION)?,
        AppMessage::Response(response)
    );
    Ok(())
}

fn payload_tag(payload: &AppRequestPayload) -> u8 {
    match payload {
        AppRequestPayload::ContinueProductRun(_) => 54,
        AppRequestPayload::QueryProductRunConversation(_) => 58,
        _ => 59,
    }
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
