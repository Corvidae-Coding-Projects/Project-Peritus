//! Product-run settlement wire compatibility and validation matrix.

use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppResponseEnvelope, AppResponsePayload, CorrelationId,
    MAX_PRODUCT_RUNS, ProductDeliverable, ProductProviderSelection, ProductRunMessageError,
    ProductRunPhase, ProductRunSettlementSnapshot, ProductRunSnapshot, ProtocolContext, ProtocolId,
    ProtocolVersion, RequestId, encode_app_message,
};
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceStatus, RunDisposition,
    SettlementCause, SettlementReducer,
};
use peritus_types::{ProviderProfileId, RunId, SessionId, Sha256Digest, WorkspaceId};

fn id<T>(
    byte: u8,
    checked: impl FnOnce([u8; 16]) -> Result<T, peritus_types::IdentifierError>,
) -> T {
    checked([byte; 16]).expect("nonzero fixture identifier")
}

fn context() -> ProtocolContext {
    ProtocolContext::new(
        id(1, ProtocolId::new),
        ProtocolVersion::new(1, 0).expect("protocol version"),
        id(2, SessionId::new),
    )
}

fn providers() -> ProductProviderSelection {
    ProductProviderSelection::new(
        id(3, ProviderProfileId::new),
        id(4, ProviderProfileId::new),
        id(5, ProviderProfileId::new),
    )
}

fn candidate_identity() -> CandidateIdentity {
    CandidateIdentity::new(
        id(6, RunId::new),
        id(7, WorkspaceId::new),
        Sha256Digest::new([8; 32]),
        3,
        1,
    )
    .expect("candidate identity")
}

fn available_snapshot() -> ProductRunSettlementSnapshot {
    let identity = candidate_identity();
    let checkpoint = CandidateCheckpoint::new(
        identity,
        CandidateStage::Changed,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
    )
    .expect("candidate checkpoint");
    let mut reducer = SettlementReducer::new();
    reducer.observe(checkpoint).expect("observe candidate");
    let settlement = reducer.settle(SettlementCause::Provider).expect("settlement");
    assert_eq!(settlement.disposition(), RunDisposition::CandidateAvailable);

    let deliverable = ProductDeliverable::candidate(
        "/managed/worktree".to_owned(),
        vec!["src/lib.rs".to_owned()],
        Vec::new(),
        "cargo test".to_owned(),
        CandidateStage::Changed,
    )
    .expect("unqualified candidate handoff");
    let snapshot = ProductRunSnapshot::new(
        identity.run_id(),
        identity.workspace_id(),
        providers(),
        ProductRunPhase::Reviewing,
        1,
        "implement settlement".to_owned(),
        "reviewer unavailable".to_owned(),
        "diff --git".to_owned(),
        String::new(),
        String::new(),
        "candidate retained".to_owned(),
    )
    .expect("product snapshot")
    .with_deliverable(deliverable);
    ProductRunSettlementSnapshot::new(snapshot, settlement).expect("settled snapshot")
}

fn response(payload: AppResponsePayload) -> AppMessage {
    AppMessage::Response(AppResponseEnvelope::new(
        context(),
        id(9, RequestId::new),
        id(10, CorrelationId::new),
        payload,
    ))
}

#[test]
fn candidate_available_round_trips_without_becoming_accepted() {
    let message = response(AppResponsePayload::ProductRunSettled(available_snapshot()));
    let bytes = encode_app_message(&message, AppProtocolLimits::PRODUCTION).expect("encode");
    let decoded = peritus_app_protocol::decode_app_message(&bytes, AppProtocolLimits::PRODUCTION)
        .expect("decode");

    assert_eq!(decoded, message);
    let AppMessage::Response(envelope) = decoded else { panic!("response") };
    let AppResponsePayload::ProductRunSettled(snapshot) = envelope.payload() else {
        panic!("settlement payload")
    };
    assert_eq!(snapshot.settlement().disposition(), RunDisposition::CandidateAvailable);
    assert_eq!(
        snapshot.snapshot().deliverable().expect("deliverable").qualification(),
        CandidateStage::Changed,
    );
    assert!(!snapshot.snapshot().deliverable().expect("deliverable").accepted());
}

#[test]
fn bounded_settlement_collection_round_trips() {
    let snapshots = vec![available_snapshot(), available_snapshot()];
    let message = response(AppResponsePayload::ProductRunSettlements(snapshots));
    let bytes = encode_app_message(&message, AppProtocolLimits::PRODUCTION).expect("encode");
    assert_eq!(
        peritus_app_protocol::decode_app_message(&bytes, AppProtocolLimits::PRODUCTION)
            .expect("decode"),
        message,
    );
}

#[test]
fn settlement_collection_rejects_the_first_out_of_bounds_item() {
    let message = response(AppResponsePayload::ProductRunSettlements(vec![
        available_snapshot();
        MAX_PRODUCT_RUNS + 1
    ]));
    assert_eq!(
        encode_app_message(&message, AppProtocolLimits::PRODUCTION)
            .expect_err("oversized settlement collection")
            .code(),
        peritus_app_protocol::AppErrorCode::LimitExceeded,
    );
}

#[test]
fn legacy_deliverable_bytes_decode_as_qualified() {
    let identity = candidate_identity();
    let deliverable = ProductDeliverable::new(
        "/managed/worktree".to_owned(),
        vec!["src/lib.rs".to_owned()],
        vec!["cargo test".to_owned()],
        "cargo test".to_owned(),
    )
    .expect("legacy deliverable");
    let snapshot = ProductRunSnapshot::new(
        identity.run_id(),
        identity.workspace_id(),
        providers(),
        ProductRunPhase::Complete,
        1,
        "implement settlement".to_owned(),
        "complete".to_owned(),
        "diff --git".to_owned(),
        "cargo test passed".to_owned(),
        "no blockers".to_owned(),
        "implemented".to_owned(),
    )
    .expect("snapshot")
    .with_deliverable(deliverable);
    let legacy = response(AppResponsePayload::ProductRuns(vec![snapshot]));
    let bytes = encode_app_message(&legacy, AppProtocolLimits::PRODUCTION).expect("legacy encode");
    let decoded = peritus_app_protocol::decode_app_message(&bytes, AppProtocolLimits::PRODUCTION)
        .expect("legacy decode");
    let AppMessage::Response(envelope) = decoded else { panic!("response") };
    let AppResponsePayload::ProductRuns(snapshots) = envelope.payload() else {
        panic!("legacy product runs")
    };
    assert_eq!(
        snapshots[0].deliverable().expect("legacy deliverable").qualification(),
        CandidateStage::Qualified,
    );
}

#[test]
fn snapshot_rejects_candidate_identity_or_stage_disagreement() {
    let available = available_snapshot();
    let mismatched = ProductDeliverable::candidate(
        "/managed/worktree".to_owned(),
        vec!["src/lib.rs".to_owned()],
        Vec::new(),
        "cargo test".to_owned(),
        CandidateStage::SelfChecked,
    )
    .expect("mismatched deliverable");
    let snapshot = available.snapshot().clone().with_deliverable(mismatched);
    assert_eq!(
        ProductRunSettlementSnapshot::new(snapshot, *available.settlement()),
        Err(ProductRunMessageError::InvalidSettlement),
    );
}
