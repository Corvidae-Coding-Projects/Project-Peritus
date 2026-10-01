//! Product-run settlement wire and validation matrix.

use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppResponseEnvelope, AppResponsePayload, CorrelationId,
    MAX_PRODUCT_RUNS, ProductDeliverable, ProductProviderSelection, ProductRunMessageError,
    ProductRunPhase, ProductRunSettlementSnapshot, ProductRunSnapshot, ProtocolContext, ProtocolId,
    ProtocolVersion, RequestId, encode_app_message,
};
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceDependencies, EvidenceRecord,
    EvidenceStatus, QualificationEvidence, RunDisposition, SettlementCause, SettlementReducer,
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
        Sha256Digest::new([8; 32]),
        None,
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
fn mixed_observations_retain_candidate_evidence_without_settling_active_work() {
    use peritus_app_protocol::ProductRunObservation;
    let candidate = available_snapshot();
    let active = ProductRunSnapshot::new(
        id(11, RunId::new),
        id(7, WorkspaceId::new),
        providers(),
        ProductRunPhase::Writing,
        1,
        "new task".to_owned(),
        "Writing".to_owned(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    )
    .expect("active");
    assert!(ProductRunObservation::new(candidate.snapshot().clone(), None).is_err());
    let observations = vec![
        ProductRunObservation::new(active, None).expect("active observation"),
        ProductRunObservation::new(candidate.snapshot().clone(), Some(*candidate.settlement()))
            .expect("candidate"),
    ];
    let message = response(AppResponsePayload::ProductRunObservations(observations.clone()));
    let bytes =
        encode_app_message(&message, AppProtocolLimits::PRODUCTION).expect("mixed list encodes");
    assert_eq!(
        peritus_app_protocol::decode_app_message(&bytes, AppProtocolLimits::PRODUCTION)
            .expect("mixed list decodes"),
        message
    );
    let oversized = response(AppResponsePayload::ProductRunObservations(vec![
        observations[0]
            .clone();
        MAX_PRODUCT_RUNS + 1
    ]));
    assert_eq!(
        encode_app_message(&oversized, AppProtocolLimits::PRODUCTION).expect_err("bounded").code(),
        peritus_app_protocol::AppErrorCode::LimitExceeded
    );
}

#[test]
fn dependency_bound_observations_round_trip_on_the_canonical_wire() {
    use peritus_app_protocol::ProductRunObservation;

    let identity = CandidateIdentity::new(
        id(6, RunId::new),
        id(7, WorkspaceId::new),
        Sha256Digest::new([0x11; 32]),
        Sha256Digest::new([0x12; 32]),
        Some(Sha256Digest::new([0x13; 32])),
        3,
        4,
    )
    .expect("observed identity");
    let passing = |dependencies| {
        EvidenceStatus::Current(EvidenceRecord::new(
            identity,
            dependencies,
            QualificationEvidence::Satisfied,
        ))
    };
    let checkpoint = CandidateCheckpoint::new(
        identity,
        CandidateStage::Qualified,
        passing(EvidenceDependencies::GATES),
        passing(EvidenceDependencies::OBLIGATIONS),
        passing(EvidenceDependencies::REVIEW),
    )
    .expect("qualified checkpoint");
    let mut reducer = SettlementReducer::new();
    reducer.observe(checkpoint).expect("observe");
    let settlement = reducer.settle(SettlementCause::Completed).expect("settlement");
    let snapshot = ProductRunSnapshot::new(
        identity.run_id(),
        identity.workspace_id(),
        providers(),
        ProductRunPhase::Complete,
        1,
        "implement bindings".to_owned(),
        "Qualified".to_owned(),
        "diff --git".to_owned(),
        "gates passed".to_owned(),
        "no blockers".to_owned(),
        "candidate retained".to_owned(),
    )
    .expect("snapshot")
    .with_deliverable(
        ProductDeliverable::candidate(
            "/managed/worktree".to_owned(),
            vec!["src/lib.rs".to_owned()],
            vec!["cargo test".to_owned()],
            "cargo test".to_owned(),
            CandidateStage::Qualified,
        )
        .expect("deliverable"),
    );
    let observation = ProductRunObservation::new(snapshot, Some(settlement)).expect("observation");
    let message = response(AppResponsePayload::ProductRunObservations(vec![observation]));
    let bytes = encode_app_message(&message, AppProtocolLimits::PRODUCTION).expect("encode");
    assert_eq!(
        peritus_app_protocol::decode_app_message(&bytes, AppProtocolLimits::PRODUCTION)
            .expect("decode"),
        message,
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
