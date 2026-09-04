use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppRequestPayload, ProductDeliverable, ProductProviderSelection,
    ProductRunControlAction, ProductRunPhase, ProductRunSettlementSnapshot, ProductRunSnapshot,
};
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceStatus, SettlementCause,
    SettlementReducer,
};
use peritus_types::{ProviderProfileId, RunId, Sha256Digest, WorkspaceId};

use super::{AppModel, context};
use crate::{
    action::{Action, Effect},
    runtime::{ProductLaunchContext, ProductProviderOption},
};

#[test]
fn unqualified_candidate_names_missing_evidence_and_requires_a_second_action() {
    let provider_id = ProviderProfileId::new([81; 16]).expect("provider");
    let workspace_id = WorkspaceId::new([82; 16]).expect("workspace");
    let product = ProductLaunchContext::new(
        workspace_id,
        "/managed/project".to_owned(),
        vec![ProductProviderOption::new(provider_id, "Codex")],
        Some(0),
    )
    .expect("product context");
    let mut model = AppModel::with_product([83; 32], Some(product));
    let _ = model.update(Action::Connected {
        context: context(),
        limits: AppProtocolLimits::PRODUCTION,
        server: "peritusd/test".to_owned(),
        downgraded: false,
    });
    let run_id = RunId::new([84; 16]).expect("run");
    let identity = CandidateIdentity::new(run_id, workspace_id, Sha256Digest::new([85; 32]), 1, 1)
        .expect("identity");
    let checkpoint = CandidateCheckpoint::new(
        identity,
        CandidateStage::Changed,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
        EvidenceStatus::Missing,
    )
    .expect("checkpoint");
    let mut reducer = SettlementReducer::new();
    reducer.observe(checkpoint).expect("observe");
    let settlement = reducer.settle(SettlementCause::Provider).expect("settle");
    let snapshot = ProductRunSnapshot::new(
        run_id,
        workspace_id,
        ProductProviderSelection::new(provider_id, provider_id, provider_id),
        ProductRunPhase::Failed,
        1,
        "build tetris".to_owned(),
        "Candidate available".to_owned(),
        "diff --git".to_owned(),
        String::new(),
        String::new(),
        "Remaining work: checks and review".to_owned(),
    )
    .expect("snapshot")
    .with_deliverable(
        ProductDeliverable::candidate(
            "/managed/project".to_owned(),
            vec!["game/src/main.rs".to_owned()],
            Vec::new(),
            "cargo run --manifest-path game/Cargo.toml".to_owned(),
            CandidateStage::Changed,
        )
        .expect("deliverable"),
    );
    model.accept_product_settlement(
        &ProductRunSettlementSnapshot::new(snapshot, settlement).expect("settled snapshot"),
    );

    let first = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::NONE,
    ))));
    assert!(first.is_empty());
    let warning =
        &model.product.as_ref().expect("product").confirmation.as_ref().expect("confirm").warning;
    assert!(warning.contains("deterministic checks missing"));
    assert!(warning.contains("independent review missing"));

    let second = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::NONE,
    ))));
    assert!(second.iter().any(|effect| matches!(
        effect,
        Effect::Send(AppMessage::Request(request))
            if matches!(request.payload(), AppRequestPayload::ControlProductRun(control)
                if control.run_id() == run_id && control.action() == ProductRunControlAction::Accept)
    )));
}
