use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppRequestPayload, ProtocolContext, ProtocolId, ProtocolVersion,
};
use peritus_types::SessionId;
use peritus_types::{ProviderProfileId, WorkspaceId};

use super::{AppModel, ConnectionStatus, View};
use crate::action::{Action, Effect};
use crate::runtime::{ProductLaunchContext, ProductProviderOption};

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
