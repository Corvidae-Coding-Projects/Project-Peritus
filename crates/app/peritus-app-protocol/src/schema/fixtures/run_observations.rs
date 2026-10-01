//! Exact candidate state alongside active work in canonical collection fixtures.
use super::values::{context, id};
use crate::{
    AppResponseEnvelope, AppResponsePayload, CorrelationId, ProductDeliverable,
    ProductProviderSelection, ProductRunPhase, ProductRunSettlementSnapshot, ProductRunSnapshot,
    RequestId,
};
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceStatus, SettlementCause,
    SettlementReducer,
};
use peritus_types::{ProviderProfileId, RunId, Sha256Digest, WorkspaceId};

pub(super) fn settled() -> AppResponseEnvelope {
    let run_id = id(31, RunId::new);
    let workspace_id = id(32, WorkspaceId::new);
    let providers = ProductProviderSelection::new(
        id(33, ProviderProfileId::new),
        id(34, ProviderProfileId::new),
        id(35, ProviderProfileId::new),
    );
    let deliverable = ProductDeliverable::candidate(
        "/worktrees/fixture".to_owned(),
        vec!["src/lib.rs".to_owned()],
        Vec::new(),
        "cargo test".to_owned(),
        CandidateStage::Changed,
    )
    .expect("fixture candidate deliverable");
    let snapshot = ProductRunSnapshot::new(
        run_id,
        workspace_id,
        providers,
        ProductRunPhase::Failed,
        1,
        "implement the requested change".to_owned(),
        "provider ended after writing a candidate".to_owned(),
        "src/lib.rs changed".to_owned(),
        String::new(),
        String::new(),
        "candidate is preserved for continuation".to_owned(),
    )
    .expect("fixture product snapshot")
    .with_deliverable(deliverable);
    let checkpoint = CandidateCheckpoint::new(
        CandidateIdentity::new(
            run_id,
            workspace_id,
            Sha256Digest::new([36; 32]),
            Sha256Digest::new([36; 32]),
            None,
            4,
            2,
        )
        .expect("fixture candidate identity"),
        CandidateStage::Changed,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
    )
    .expect("fixture candidate checkpoint");
    let mut reducer = SettlementReducer::new();
    reducer.observe(checkpoint).expect("fixture candidate observation");
    let settlement = reducer.settle(SettlementCause::Provider).expect("fixture settlement");
    let settled = ProductRunSettlementSnapshot::new(snapshot, settlement)
        .expect("fixture product settlement snapshot");
    AppResponseEnvelope::new(
        context(),
        id(10, RequestId::new),
        id(11, CorrelationId::new),
        AppResponsePayload::ProductRunSettled(settled),
    )
}

pub(super) fn mixed() -> AppResponseEnvelope {
    let settled_response = settled();
    let AppResponsePayload::ProductRunSettled(settled) = settled_response.payload() else {
        unreachable!("settlement fixture")
    };
    let active = ProductRunSnapshot::new(
        id(91, RunId::new),
        settled.snapshot().workspace_id(),
        settled.snapshot().providers(),
        ProductRunPhase::Writing,
        1,
        "active task".to_owned(),
        "Working".to_owned(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    )
    .expect("active snapshot");
    AppResponseEnvelope::new(
        context(),
        id(92, RequestId::new),
        id(93, CorrelationId::new),
        AppResponsePayload::ProductRunObservations(vec![
            crate::ProductRunObservation::new(active, None).expect("active observation"),
            crate::ProductRunObservation::new(
                settled.snapshot().clone(),
                Some(*settled.settlement()),
            )
            .expect("candidate observation"),
        ]),
    )
}
