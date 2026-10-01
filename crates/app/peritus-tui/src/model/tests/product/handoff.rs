use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use peritus_app_protocol::{AppMessage, AppProtocolLimits, AppRequestPayload};
use peritus_run_settlement::CandidateStage;
use peritus_types::{ProviderProfileId, WorkspaceId};

use super::super::{AppModel, context};
use crate::{
    action::{Action, Effect},
    model::View,
    runtime::{ProductLaunchContext, ProductProviderOption},
};

#[test]
fn product_launch_queries_runs_and_retired_task_key_is_inert() {
    let product = ProductLaunchContext::new(
        WorkspaceId::new([41; 16]).expect("workspace"),
        "/managed/project".to_owned(),
        vec![ProductProviderOption::new(
            ProviderProfileId::new([42; 16]).expect("provider"),
            "Codex",
        )],
        Some(0),
    )
    .expect("product context");
    let mut model = AppModel::with_product([43; 32], Some(product));
    model.view = View::Runs;
    let effects = model.update(Action::Connected {
        context: context(),
        limits: AppProtocolLimits::PRODUCTION,
        server: "peritusd/test".to_owned(),
        downgraded: false,
    });
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Send(AppMessage::Request(request)) if matches!(request.payload(), AppRequestPayload::QueryProductRunObservations(_)))));
    let effects = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('n'),
        KeyModifiers::NONE,
    ))));
    assert!(effects.is_empty());
    assert!(model.editor.is_none());
    assert_eq!(model.view, View::Runs);
}

#[test]
fn selected_product_run_accepts_conversational_followup() {
    use peritus_app_protocol::{ProductProviderSelection, ProductRunPhase, ProductRunSnapshot};
    use peritus_types::RunId;

    let provider_id = ProviderProfileId::new([61; 16]).expect("provider");
    let workspace_id = WorkspaceId::new([62; 16]).expect("workspace");
    let product = ProductLaunchContext::new(
        workspace_id,
        "/managed/project".to_owned(),
        vec![ProductProviderOption::new(provider_id, "Codex")],
        Some(0),
    )
    .expect("product context");
    let mut model = AppModel::with_product([63; 32], Some(product));
    model.view = View::Runs;
    let _ = model.update(Action::Connected {
        context: context(),
        limits: AppProtocolLimits::PRODUCTION,
        server: "peritusd/test".to_owned(),
        downgraded: false,
    });
    let run_id = RunId::new([64; 16]).expect("run");
    model.accept_product_run(
        ProductRunSnapshot::new(
            run_id,
            workspace_id,
            ProductProviderSelection::new(provider_id, provider_id, provider_id),
            ProductRunPhase::Failed,
            1,
            "build tetris".to_owned(),
            "plan failed".to_owned(),
            String::new(),
            String::new(),
            String::new(),
            "invalid JSON".to_owned(),
            crate::test_support::run_operation(run_id, ProductRunPhase::Failed),
        )
        .expect("snapshot"),
    );
    assert!(
        model
            .update(Action::TerminalEvent(Event::Key(KeyEvent::new(
                KeyCode::Char('m'),
                KeyModifiers::NONE,
            ))))
            .is_empty()
    );
    for character in "/runs".chars() {
        let _ = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::NONE,
        ))));
    }
    assert!(
        model
            .update(Action::TerminalEvent(Event::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))))
            .is_empty(),
        "local dashboard navigation must not become a product-run continuation",
    );
    assert_eq!(model.view, View::Runs);
    assert!(model.editor.is_none());
    assert!(
        model
            .update(Action::TerminalEvent(Event::Key(KeyEvent::new(
                KeyCode::Char('m'),
                KeyModifiers::NONE,
            ))))
            .is_empty()
    );
    for character in "continue and use ratatui".chars() {
        let _ = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::NONE,
        ))));
    }
    let effects = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    ))));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Send(AppMessage::Request(request))
            if matches!(request.payload(), AppRequestPayload::QueryInteractionBinding(_))
    )));
}

#[test]
fn completed_product_run_exposes_all_four_handoff_controls() {
    use peritus_app_protocol::{
        ProductDeliverable, ProductProviderSelection, ProductRunControlAction, ProductRunPhase,
        ProductRunSnapshot,
    };
    use peritus_types::RunId;

    for (key, expected) in [
        ('a', ProductRunControlAction::Accept),
        ('c', ProductRunControlAction::Commit),
        ('p', ProductRunControlAction::Export),
        ('D', ProductRunControlAction::Discard),
    ] {
        let provider_id = ProviderProfileId::new([71; 16]).expect("provider");
        let workspace_id = WorkspaceId::new([72; 16]).expect("workspace");
        let product = ProductLaunchContext::new(
            workspace_id,
            "/managed/project".to_owned(),
            vec![ProductProviderOption::new(provider_id, "Codex")],
            Some(0),
        )
        .expect("product context");
        let mut model = AppModel::with_product([73; 32], Some(product));
        model.view = View::Runs;
        let _ = model.update(Action::Connected {
            context: context(),
            limits: AppProtocolLimits::PRODUCTION,
            server: "peritusd/test".to_owned(),
            downgraded: false,
        });
        let run_id = RunId::new([74; 16]).expect("run");
        model.accept_product_run(super::with_controls(
            ProductRunSnapshot::new(
                run_id,
                workspace_id,
                ProductProviderSelection::new(provider_id, provider_id, provider_id),
                ProductRunPhase::Complete,
                1,
                "build tetris".to_owned(),
                "passing".to_owned(),
                "diff --git".to_owned(),
                "cargo test: PASS".to_owned(),
                "No findings".to_owned(),
                "completed".to_owned(),
                crate::test_support::run_operation(run_id, ProductRunPhase::Complete),
            )
            .expect("snapshot")
            .with_deliverable(
                ProductDeliverable::candidate(
                    "/managed/project".to_owned(),
                    vec!["game/src/main.rs".to_owned()],
                    vec!["cargo test --manifest-path game/Cargo.toml".to_owned()],
                    "cargo run --manifest-path game/Cargo.toml".to_owned(),
                    CandidateStage::Qualified,
                )
                .expect("deliverable"),
            ),
            peritus_app_protocol::ProductRunOperationState::Succeeded,
            peritus_app_protocol::ProductRunLegalControls::none()
                .with(ProductRunControlAction::Accept)
                .with(ProductRunControlAction::Commit)
                .with(ProductRunControlAction::Export)
                .with(ProductRunControlAction::Discard),
        ));

        let effects = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
            KeyCode::Char(key),
            KeyModifiers::NONE,
        ))));
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::Send(AppMessage::Request(request))
                if matches!(
                    request.payload(),
                    AppRequestPayload::ControlProductRun(control)
                        if control.run_id() == run_id && control.action() == expected
                )
        )));
    }
}

#[test]
fn unknown_command_outcome_exposes_acknowledgement_without_retry() {
    use peritus_app_protocol::{
        ProductProviderSelection, ProductRunControlAction, ProductRunLegalControls,
        ProductRunOperation, ProductRunOperationKind, ProductRunOperationState, ProductRunPhase,
        ProductRunSnapshot,
    };
    use peritus_types::RunId;

    let provider = ProviderProfileId::new([75; 16]).expect("provider");
    let workspace = WorkspaceId::new([76; 16]).expect("workspace");
    let product = ProductLaunchContext::new(
        workspace,
        "/managed/project".to_owned(),
        vec![ProductProviderOption::new(provider, "Codex")],
        Some(0),
    )
    .expect("product context");
    let mut model = AppModel::with_product([77; 32], Some(product));
    model.view = View::Runs;
    let _ = model.update(Action::Connected {
        context: context(),
        limits: AppProtocolLimits::PRODUCTION,
        server: "peritusd/test".to_owned(),
        downgraded: false,
    });
    let run = RunId::new([78; 16]).expect("run");
    let snapshot = ProductRunSnapshot::new(
        run,
        workspace,
        ProductProviderSelection::new(provider, provider, provider),
        ProductRunPhase::RecoveryRequired,
        1,
        "run a command".to_owned(),
        "interrupted".to_owned(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        crate::test_support::run_operation(run, ProductRunPhase::RecoveryRequired),
    )
    .expect("snapshot")
    .with_operation(
        ProductRunOperation::new(
            ProductRunOperationKind::Command,
            ProductRunOperationState::OutcomeUnknown,
            "command/test".to_owned(),
            "The receipt is retained.".to_owned(),
            "The host cannot prove whether it took effect.".to_owned(),
            ProductRunLegalControls::none().with(ProductRunControlAction::Acknowledge),
        )
        .expect("operation"),
    );
    model.accept_product_run(snapshot);

    assert!(model.control_selected_product_run(ProductRunControlAction::Retry).is_empty());
    let effects = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('u'),
        KeyModifiers::NONE,
    ))));
    assert!(effects.iter().any(|effect| matches!(
        effect,
        Effect::Send(AppMessage::Request(request))
            if matches!(request.payload(), AppRequestPayload::ControlProductRun(control)
                if control.run_id() == run
                    && control.action() == ProductRunControlAction::Acknowledge)
    )));
}

#[test]
fn completed_product_run_exposes_foreground_run_action() {
    use peritus_app_protocol::{
        ProductDeliverable, ProductProviderSelection, ProductRunPhase,
        ProductRunSettlementSnapshot, ProductRunSnapshot,
    };
    use peritus_run_settlement::{
        CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceDependencies,
        EvidenceRecord, EvidenceStatus, QualificationEvidence, SettlementCause, SettlementReducer,
    };
    use peritus_types::{RunId, Sha256Digest};

    let provider_id = ProviderProfileId::new([75; 16]).expect("provider");
    let workspace_id = WorkspaceId::new([76; 16]).expect("workspace");
    let product = ProductLaunchContext::new(
        workspace_id,
        "/managed/project".to_owned(),
        vec![ProductProviderOption::new(provider_id, "Codex")],
        Some(0),
    )
    .expect("product context");
    let mut model = AppModel::with_product([77; 32], Some(product));
    model.view = View::Runs;
    let run_id = RunId::new([78; 16]).expect("run");
    let digest = Sha256Digest::new([79; 32]);
    let identity =
        CandidateIdentity::new(run_id, workspace_id, digest, digest, None, 1, 1).expect("identity");
    let passed = EvidenceStatus::Current(EvidenceRecord::new(
        identity,
        EvidenceDependencies::OBLIGATIONS,
        QualificationEvidence::Satisfied,
    ));
    let checkpoint =
        CandidateCheckpoint::new(identity, CandidateStage::Qualified, passed, passed, passed)
            .expect("checkpoint");
    let mut reducer = SettlementReducer::new();
    reducer.observe(checkpoint).expect("checkpoint observation");
    let settlement = reducer.settle(SettlementCause::Completed).expect("settlement");
    let snapshot = ProductRunSnapshot::new(
        run_id,
        workspace_id,
        ProductProviderSelection::new(provider_id, provider_id, provider_id),
        ProductRunPhase::Complete,
        1,
        "build tetris".to_owned(),
        "passing".to_owned(),
        String::new(),
        String::new(),
        String::new(),
        "completed".to_owned(),
        crate::test_support::run_operation(run_id, ProductRunPhase::Complete),
    )
    .expect("snapshot")
    .with_deliverable(
        ProductDeliverable::candidate(
            "/managed/project".to_owned(),
            vec!["game/src/main.rs".to_owned()],
            vec!["cargo test --manifest-path game/Cargo.toml".to_owned()],
            "cargo run --manifest-path game/Cargo.toml".to_owned(),
            CandidateStage::Qualified,
        )
        .expect("deliverable"),
    );
    model.accept_product_settlement(
        &ProductRunSettlementSnapshot::new(snapshot, settlement).expect("settled snapshot"),
    );

    let effects = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('v'),
        KeyModifiers::NONE,
    ))));
    assert!(matches!(
        effects.as_slice(),
        [Effect::RunCandidate { workspace, instruction, candidate_digest }]
            if workspace == std::path::Path::new("/managed/project")
                && instruction == "cargo run --manifest-path game/Cargo.toml"
                && *candidate_digest == digest
    ));
}
