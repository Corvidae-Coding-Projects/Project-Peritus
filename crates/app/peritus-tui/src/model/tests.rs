use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppRequestPayload, ProtocolContext, ProtocolId, ProtocolVersion,
};
use peritus_types::SessionId;
use peritus_types::{ProviderProfileId, WorkspaceId};

use super::{AppModel, ConnectionStatus, View};
use crate::action::{Action, Effect};
use crate::runtime::{ProductLaunchContext, ProductProviderOption};

mod editor;
mod navigation;
mod notice;
mod product;
mod prompts;
mod terminal;

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
fn a_reestablished_session_is_visible_to_the_user() {
    let mut model = AppModel::new([12; 32]);
    for _ in 0..2 {
        let _ = model.update(Action::Connected {
            context: context(),
            limits: AppProtocolLimits::PRODUCTION,
            server: "peritusd/test".to_owned(),
            downgraded: false,
        });
    }

    assert!(model.notice.as_ref().is_some_and(|notice| notice.text == "reconnected to daemon"));
    assert_eq!(model.connection_generation(), 2);
}

#[test]
fn disconnect_drops_only_live_connection_state_and_retains_session_and_cursor() {
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
    assert_eq!(model.retained_session(), Some(context().session_id()));
    assert_eq!(model.last_cursor().get(), 0);
}

#[test]
fn unanswered_requests_remain_pending_without_forcing_a_new_connection() {
    let mut model = AppModel::new([10; 32]);
    let _ = model.update(Action::Connected {
        context: context(),
        limits: AppProtocolLimits::PRODUCTION,
        server: "peritusd/test".to_owned(),
        downgraded: false,
    });
    let original = model.pending.keys().copied().collect::<Vec<_>>();
    let now = std::time::Instant::now();

    let mut effects = Vec::new();
    for step in 1..=120 {
        effects = model.update(Action::Tick(now + std::time::Duration::from_hours(step)));
    }

    assert!(original.iter().all(|request| model.pending.contains_key(request)));
    assert!(!effects.iter().any(|effect| matches!(effect, Effect::Reconnect)));
    assert!(matches!(model.connection, ConnectionStatus::Online { .. }));
}
