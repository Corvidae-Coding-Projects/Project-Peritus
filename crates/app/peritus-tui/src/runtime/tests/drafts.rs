use super::*;
use crate::model::{Editor, EditorKind, View};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use peritus_app_protocol::{AppProtocolLimits, CURRENT_PROTOCOL_VERSION, ProtocolContext};
use peritus_types::RunId;

fn config() -> TuiConfig {
    TuiConfig::new("fixture.sock").with_product(
        ProductLaunchContext::new(
            WorkspaceId::new([41; 16]).unwrap(),
            "fixture".to_owned(),
            vec![ProductProviderOption::new(ProviderProfileId::new([42; 16]).unwrap(), "fixture")],
            Some(0),
        )
        .unwrap(),
    )
}

#[test]
fn launcher_recovery_preserves_the_open_message_modal_and_its_cursor() {
    let config = config();
    let mut state = TuiState::default();
    let mut model = state.take_model(&config, [43; 32]);
    model.editor = Some(Editor {
        kind: EditorKind::ProductMessage(RunId::new([40; 16]).unwrap()),
        title: "Message",
        hint: "fixture",
        buffer: String::new(),
        cursor: 0,
        pasted_command: false,
    });
    model.update(Action::TerminalEvent(Event::Paste("unfinished task λ".to_owned())));
    let cursor = model.editor.as_ref().unwrap().cursor;
    let effects = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('r'),
        KeyModifiers::CONTROL,
    ))));
    assert!(matches!(effects.as_slice(), [Effect::Reconnect]));
    state.retain(config.clone(), model);
    let restored = state.take_model(&config, [44; 32]);
    let editor = restored.editor.as_ref().expect("retained message editor");
    assert_eq!(editor.buffer, "unfinished task λ");
    assert_eq!(editor.cursor, cursor);
    assert!(matches!(editor.kind, EditorKind::ProductMessage(_)));
    state.retain(config, restored);
    let different = state.take_model(&TuiConfig::new("other.sock"), [45; 32]);
    assert!(different.editor.is_none());
}

#[test]
fn launcher_recovery_preserves_ambiguous_message_without_replaying_it() {
    let config = config();
    let mut state = TuiState::default();
    let mut model = state.take_model(&config, [46; 32]);
    let context = ProtocolContext::new(
        ProtocolId::new([47; 16]).unwrap(),
        CURRENT_PROTOCOL_VERSION,
        SessionId::new([48; 16]).unwrap(),
    );
    let connected = || Action::Connected {
        context,
        limits: AppProtocolLimits::PRODUCTION,
        server: "fixture".into(),
        downgraded: false,
    };
    model.update(connected());
    model.view = View::Runs;
    model.editor = Some(Editor {
        kind: EditorKind::ProductMessage(RunId::new([50; 16]).unwrap()),
        title: "Message",
        hint: "fixture",
        buffer: "already sent once".into(),
        cursor: 17,
        pasted_command: false,
    });
    let sent = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Enter,
        KeyModifiers::NONE,
    ))));
    assert!(sent.iter().any(|effect| matches!(effect, Effect::Send(_))));
    assert!(model.editor.is_none());
    state.retain(config.clone(), model);
    let mut restored = state.take_model(&config, [49; 32]);
    let editor = restored.editor.as_ref().expect("uncertain message draft");
    assert_eq!(editor.buffer, "already sent once");
    assert!(editor.hint.contains("may already have been accepted"));
    let effects = restored.update(connected());
    assert!(!effects.iter().any(|effect| matches!(effect,
        Effect::Send(peritus_app_protocol::AppMessage::Request(request)) if matches!(request.payload(), peritus_app_protocol::AppRequestPayload::ContinueProductRun(_))
    )));
}

#[test]
fn chat_reconnect_keeps_the_message_without_requiring_it_to_be_replaced_by_a_command() {
    let config = config();
    let mut state = TuiState::default();
    let mut model = state.take_model(&config, [51; 32]);
    model.update(Action::TerminalEvent(Event::Paste("Please keep this unsent request λ".into())));
    let cursor = model.chat.cursor;
    let effects = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('r'),
        KeyModifiers::CONTROL,
    ))));
    assert!(matches!(effects.as_slice(), [Effect::Reconnect]));
    state.retain(config.clone(), model);
    let restored = state.take_model(&config, [52; 32]);
    assert_eq!(restored.chat.buffer, "Please keep this unsent request λ");
    assert_eq!(restored.chat.cursor, cursor);
    assert!(restored.chat.run_id.is_none());
}

#[test]
fn control_r_reconnects_from_every_product_panel() {
    let config = config();
    for view in View::ALL {
        let mut model = AppModel::with_product([53; 32], config.product().cloned());
        model.view = view;
        let effects = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
            KeyCode::Char('r'),
            KeyModifiers::CONTROL,
        ))));
        assert!(matches!(effects.as_slice(), [Effect::Reconnect]), "no reconnect in {view:?}");
    }
}
