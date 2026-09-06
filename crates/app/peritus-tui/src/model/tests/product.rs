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
    model.view = crate::model::View::Runs;
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

#[test]
fn exact_run_poll_does_not_replace_the_list_or_undo_navigation() {
    use peritus_app_protocol::{AppResponseEnvelope, AppResponsePayload};
    for settled_response in [false, true] {
        let provider = ProviderProfileId::new([81; 16]).expect("provider");
        let workspace = WorkspaceId::new([82; 16]).expect("workspace");
        let launch = ProductLaunchContext::new(
            workspace,
            "/managed/project".to_owned(),
            vec![ProductProviderOption::new(provider, "Codex")],
            Some(0),
        )
        .expect("launch");
        let snapshots = [84, 85, 86].map(|id| {
            ProductRunSnapshot::new(
                RunId::new([id; 16]).expect("run"),
                workspace,
                ProductProviderSelection::new(provider, provider, provider),
                ProductRunPhase::Failed,
                1,
                format!("task {id}"),
                "Failed".to_owned(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            )
            .expect("snapshot")
        });
        let mut model = AppModel::with_product([83; 32], Some(launch));
        model.context = Some(context());
        model.accept_product_runs(snapshots.to_vec());
        let effects = model.poll_product_runs();
        assert!(model.select_next_product());
        for effect in effects {
            let Effect::Send(AppMessage::Request(request)) = effect else { continue };
            let AppRequestPayload::QueryProductRuns(query) = request.payload() else { continue };
            let payload = if query.run_id().is_none() {
                AppResponsePayload::ProductRuns(snapshots.to_vec())
            } else if settled_response {
                let settlement =
                    SettlementReducer::new().settle(SettlementCause::Provider).expect("settlement");
                AppResponsePayload::ProductRunSettlements(vec![
                    ProductRunSettlementSnapshot::new(snapshots[0].clone(), settlement)
                        .expect("settled snapshot"),
                ])
            } else {
                AppResponsePayload::ProductRuns(vec![snapshots[0].clone()])
            };
            let _ = model.update(Action::Message(AppMessage::Response(AppResponseEnvelope::new(
                request.context(),
                request.request_id(),
                request.correlation_id(),
                payload,
            ))));
            let product = model.product.as_ref().expect("product");
            assert_eq!(product.runs.len(), 3);
            assert_eq!(product.selected_run().expect("selected").run_id(), snapshots[1].run_id());
        }
    }
}
