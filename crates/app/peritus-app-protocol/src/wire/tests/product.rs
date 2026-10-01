//! Product-run request, control, conversation, and snapshot wire coverage.

use crate::{
    AppProtocolLimits, AppRequestEnvelope, AppRequestPayload, AppResponseEnvelope,
    AppResponsePayload, CorrelationId, ProductDeliverable, ProductProviderSelection,
    ProductRunControl, ProductRunControlAction, ProductRunLegalControls, ProductRunObservation,
    ProductRunOperation, ProductRunOperationKind, ProductRunOperationState, ProductRunPhase,
    ProductRunSnapshot, ProtocolContext, ProtocolId, ProtocolVersion, RequestId,
};
use peritus_run_settlement::CandidateStage;
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
fn product_run_snapshots_round_trip() -> Result<(), Box<dyn std::error::Error>> {
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
    .with_operation(ProductRunOperation::new(
        ProductRunOperationKind::Command,
        ProductRunOperationState::OutcomeUnknown,
        "command/receipt-36".to_owned(),
        "The original command receipt is retained.".to_owned(),
        "The host cannot prove whether it took effect.".to_owned(),
        ProductRunLegalControls::none().with(ProductRunControlAction::Acknowledge),
    )?)
    .with_deliverable(
        ProductDeliverable::candidate(
            "/managed/worktree".to_owned(),
            vec!["game/src/main.rs".to_owned()],
            vec![
                "cargo test --manifest-path game/Cargo.toml --all-targets --all-features"
                    .to_owned(),
            ],
            "cargo run --manifest-path game/Cargo.toml".to_owned(),
            CandidateStage::Qualified,
        )?
        .mark_accepted()
        .mark_exported("/state/exports/run.patch".to_owned())?,
    );
    let response = AppResponseEnvelope::new(
        context,
        RequestId::new([40; 16]).expect("nonzero request id"),
        CorrelationId::new([41; 16]).expect("nonzero correlation id"),
        AppResponsePayload::ProductRunObservations(vec![ProductRunObservation::new(
            snapshot, None,
        )?]),
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
fn all_product_controls_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let context = ProtocolContext::new(
        ProtocolId::new([42; 16]).expect("protocol"),
        ProtocolVersion::new(1, 0)?,
        SessionId::new([43; 16]).expect("session"),
    );
    let run_id = RunId::new([44; 16]).expect("run");
    for (index, action) in [
        ProductRunControlAction::Cancel,
        ProductRunControlAction::Retry,
        ProductRunControlAction::Accept,
        ProductRunControlAction::Commit,
        ProductRunControlAction::Export,
        ProductRunControlAction::Discard,
        ProductRunControlAction::Acknowledge,
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
