use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppRequestPayload, ProtocolContext, ProtocolId, ProtocolVersion,
};
use peritus_types::SessionId;
use peritus_types::{ProviderProfileId, WorkspaceId};

use super::{AppModel, ConnectionStatus, View};
use crate::action::{Action, Effect};
use crate::runtime::{ProductLaunchContext, ProductProviderOption};

mod product;

fn context() -> ProtocolContext {
    ProtocolContext::new(
        ProtocolId::new([1; 16]).expect("protocol"),
        ProtocolVersion::new(1, 0).expect("version"),
        SessionId::new([2; 16]).expect("session"),
    )
}

#[test]
fn connection_starts_status_and_resumable_event_requests() {
    let mut model = AppModel::new([7; 32]);
    let effects = model.update(Action::Connected {
        context: context(),
        limits: AppProtocolLimits::PRODUCTION,
        server: "peritusd/test".to_owned(),
        downgraded: false,
    });
    assert!(matches!(
        model.connection,
        ConnectionStatus::Online { ref server, downgraded: false } if server == "peritusd/test"
    ));
    assert_eq!(effects.len(), 2);
    let payloads = effects
        .iter()
        .map(|effect| match effect {
            Effect::Send(AppMessage::Request(request)) => request.payload(),
            _ => panic!("connection emitted a non-request effect"),
        })
        .collect::<Vec<_>>();
    assert!(payloads.iter().any(|payload| matches!(payload, AppRequestPayload::DaemonStatus)));
    assert!(payloads.iter().any(|payload| matches!(payload, AppRequestPayload::Subscribe(_))));
    assert_eq!(model.retained_session(), Some(context().session_id()));
}

#[test]
fn navigation_reconnect_and_quit_are_deterministic() {
    let mut model = AppModel::new([8; 32]);
    assert!(
        model
            .update(Action::TerminalEvent(Event::Key(KeyEvent::new(
                KeyCode::Char('4'),
                KeyModifiers::NONE,
            ))))
            .is_empty()
    );
    assert_eq!(model.view, View::Trace);

    assert!(matches!(
        model
            .update(Action::TerminalEvent(Event::Key(KeyEvent::new(
                KeyCode::Char('r'),
                KeyModifiers::NONE,
            ))))
            .as_slice(),
        [Effect::Reconnect]
    ));
    assert!(matches!(model.connection, ConnectionStatus::Connecting));

    assert!(matches!(
        model
            .update(Action::TerminalEvent(Event::Key(KeyEvent::new(
                KeyCode::Char('q'),
                KeyModifiers::CONTROL,
            ))))
            .as_slice(),
        [Effect::Quit]
    ));
    assert!(model.quitting);
}

#[test]
fn disconnect_drops_only_live_connection_state_and_retains_cursor() {
    let mut model = AppModel::new([9; 32]);
    let _ = model.update(Action::Connected {
        context: context(),
        limits: AppProtocolLimits::PRODUCTION,
        server: "peritusd/test".to_owned(),
        downgraded: false,
    });
    assert!(model.update(Action::Disconnected("socket closed".to_owned())).is_empty());
    assert!(matches!(
        model.connection,
        ConnectionStatus::Disconnected(ref detail) if detail == "socket closed"
    ));
    assert_eq!(model.retained_session(), None);
    assert_eq!(model.last_cursor().get(), 0);
}

#[test]
fn product_launch_queries_runs_and_task_submission_is_daemon_owned() {
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
    let effects = model.update(Action::Connected {
        context: context(),
        limits: AppProtocolLimits::PRODUCTION,
        server: "peritusd/test".to_owned(),
        downgraded: false,
    });
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Send(AppMessage::Request(request)) if matches!(request.payload(), AppRequestPayload::QueryProductRuns(_)))));
    let effects = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('n'),
        KeyModifiers::NONE,
    ))));
    assert!(effects.is_empty());
    assert!(model.editor.is_some());
    for character in "add a status command".chars() {
        let _ = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
            KeyCode::Char(character),
            KeyModifiers::NONE,
        ))));
    }
    let effects = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    ))));
    assert!(effects.iter().any(|effect| matches!(effect, Effect::Send(AppMessage::Request(request)) if matches!(request.payload(), AppRequestPayload::StartProductRun(_)))));
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
        )
        .expect("snapshot"),
    );
    assert!(
        model
            .update(Action::TerminalEvent(Event::Key(KeyEvent::new(
                KeyCode::Enter,
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
            if matches!(request.payload(), AppRequestPayload::ContinueProductRun(_))
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
        let _ = model.update(Action::Connected {
            context: context(),
            limits: AppProtocolLimits::PRODUCTION,
            server: "peritusd/test".to_owned(),
            downgraded: false,
        });
        let run_id = RunId::new([74; 16]).expect("run");
        model.accept_product_run(
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
            )
            .expect("snapshot")
            .with_deliverable(
                ProductDeliverable::new(
                    "/managed/project".to_owned(),
                    vec!["game/src/main.rs".to_owned()],
                    vec!["cargo test --manifest-path game/Cargo.toml".to_owned()],
                    "cargo run --manifest-path game/Cargo.toml".to_owned(),
                )
                .expect("deliverable"),
            ),
        );

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
fn completed_product_run_exposes_foreground_run_action() {
    use peritus_app_protocol::{
        ProductDeliverable, ProductProviderSelection, ProductRunPhase,
        ProductRunSettlementSnapshot, ProductRunSnapshot,
    };
    use peritus_run_settlement::{
        CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceRecord, EvidenceStatus,
        QualificationEvidence, SettlementCause, SettlementReducer,
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
    let run_id = RunId::new([78; 16]).expect("run");
    let digest = Sha256Digest::new([79; 32]);
    let identity = CandidateIdentity::new(run_id, workspace_id, digest, 1, 1).expect("identity");
    let passed =
        EvidenceStatus::Current(EvidenceRecord::new(identity, QualificationEvidence::Satisfied));
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
    )
    .expect("snapshot")
    .with_deliverable(
        ProductDeliverable::new(
            "/managed/project".to_owned(),
            vec!["game/src/main.rs".to_owned()],
            vec!["cargo test --manifest-path game/Cargo.toml".to_owned()],
            "cargo run --manifest-path game/Cargo.toml".to_owned(),
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
