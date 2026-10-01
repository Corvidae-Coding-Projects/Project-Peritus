//! Captured keys belong to the child until the explicit release chord.
use super::*;
use crate::model::EditorKind;
use peritus_app_protocol::{RequestId, TerminalAttachmentId, TerminalBinding};
use peritus_types::ProcessId;

mod pipes;

#[test]
fn interrupt_escape_and_both_release_key_encodings_obey_capture() {
    let mut model = attached_model();
    for (key, bytes) in [
        (KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL), vec![3]),
        (KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL), vec![18]),
        (KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE), vec![27]),
    ] {
        let effects = model.update(Action::TerminalEvent(Event::Key(key)));
        assert!(matches!(effects.as_slice(), [Effect::Send(AppMessage::Request(request))]
            if matches!(request.payload(), AppRequestPayload::TerminalInput(input) if input.bytes() == bytes)));
        assert!(!model.quitting);
        assert_eq!(model.view, View::Terminal);
    }
    for character in [']', '5'] {
        model.terminal.as_mut().expect("terminal").set_capture_input(true);
        assert!(
            model
                .update(Action::TerminalEvent(Event::Key(KeyEvent::new(
                    KeyCode::Char(character),
                    KeyModifiers::CONTROL
                ))))
                .is_empty()
        );
        assert!(!model.terminal.as_ref().expect("terminal").capture_input());
    }
    model
        .update(Action::TerminalEvent(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))));
    assert_eq!(model.view, View::Conversation);
}

#[test]
fn a_hidden_terminal_cannot_receive_paste_intended_for_another_panel() {
    let mut model = attached_model();
    for view in [View::Approvals, View::Help, View::Runs, View::Diff, View::Review] {
        model.view = view;
        let effects = model.update(Action::TerminalEvent(Event::Paste("private answer".into())));
        assert!(effects.is_empty(), "hidden terminal received paste in {view:?}");
    }
    model.view = View::Terminal;
    let effects = model.update(Action::TerminalEvent(Event::Paste("visible input".into())));
    assert!(matches!(effects.as_slice(), [Effect::Send(AppMessage::Request(request))]
        if matches!(request.payload(), AppRequestPayload::TerminalInput(input) if input.bytes() == b"visible input")));
}

#[test]
fn terminal_input_honors_the_childs_paste_and_cursor_modes() {
    use peritus_app_protocol::{TerminalOutput, TerminalStream};
    let mut model = attached_model();
    let terminal = model.terminal.as_mut().unwrap();
    let enabled = b"\x1b[?2004h\x1b[?1h";
    terminal
        .accept_output(
            &TerminalOutput::new(
                terminal.binding(),
                0,
                0,
                TerminalStream::Terminal,
                enabled.to_vec(),
                8192,
            )
            .unwrap(),
        )
        .unwrap();
    let paste = model.update(Action::TerminalEvent(Event::Paste("first\nsecond".into())));
    assert!(matches!(paste.as_slice(), [Effect::Send(AppMessage::Request(request))]
        if matches!(request.payload(), AppRequestPayload::TerminalInput(input)
            if input.bytes() == b"\x1b[200~first\nsecond\x1b[201~")));
    let arrow = model
        .update(Action::TerminalEvent(Event::Key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))));
    assert!(matches!(arrow.as_slice(), [Effect::Send(AppMessage::Request(request))]
        if matches!(request.payload(), AppRequestPayload::TerminalInput(input) if input.bytes() == b"\x1bOA")));
    let terminal = model.terminal.as_mut().unwrap();
    terminal
        .accept_output(
            &TerminalOutput::new(
                terminal.binding(),
                1,
                enabled.len() as u64,
                TerminalStream::Terminal,
                b"\x1b[?2004l\x1b[?1l".to_vec(),
                8192,
            )
            .unwrap(),
        )
        .unwrap();
    let paste = model.update(Action::TerminalEvent(Event::Paste("plain".into())));
    assert!(matches!(paste.as_slice(), [Effect::Send(AppMessage::Request(request))]
        if matches!(request.payload(), AppRequestPayload::TerminalInput(input) if input.bytes() == b"plain")));
}

#[test]
fn terminal_lifecycle_exit_releases_navigation_and_allows_another_attachment() {
    use peritus_app_protocol::{TerminalExit, TerminalExitDisposition};
    let mut model = attached_model();
    let terminal = model.terminal.as_mut().unwrap();
    terminal
        .accept_exit(TerminalExit::new(terminal.binding(), 0, 0, TerminalExitDisposition::Code(0)))
        .unwrap();
    model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('1'),
        KeyModifiers::NONE,
    ))));
    assert_eq!(model.view, View::Runs, "a completed child must release the user's keyboard");
    model.view = View::Terminal;
    model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::NONE,
    ))));
    assert!(
        model.editor.as_ref().is_some_and(|editor| matches!(editor.kind, EditorKind::ProcessId))
    );
}

#[test]
fn terminal_lifecycle_disconnect_leaves_reconnect_key_reachable() {
    let mut model = attached_model();
    model.update(Action::Disconnected("connection lost".into()));
    let effects = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('R'),
        KeyModifiers::NONE,
    ))));
    assert!(
        matches!(effects.as_slice(), [Effect::Reconnect]),
        "a disconnected terminal must not swallow reconnect"
    );
}

#[test]
fn unavailable_output_releases_capture_without_disconnecting_chat_or_claiming_exit() {
    use peritus_app_protocol::{AppEventEnvelope, AppEventPayload, TerminalPhase};
    let mut model = attached_model();
    let original_context = model.context;
    let binding = model.terminal.as_ref().unwrap().binding();
    let stale = TerminalBinding::new(
        TerminalAttachmentId::new([51; 16]).unwrap(),
        binding.process_id(),
        binding.originating_request_id(),
    );
    for (failed, unavailable) in [(stale, false), (binding, true)] {
        let effects = model.update(Action::Message(AppMessage::Event(AppEventEnvelope::new(
            context(),
            AppEventPayload::TerminalUnavailable(failed),
        ))));
        assert!(effects.is_empty());
        let terminal = model.terminal.as_ref().unwrap();
        assert_eq!(terminal.can_capture(), !unavailable);
        assert_eq!(terminal.phase(), TerminalPhase::Attached, "no fabricated process exit");
        assert_eq!(model.context, original_context, "chat connection survives");
    }
    assert!(model.terminal.as_ref().unwrap().phase_label().contains("Output unavailable"));
    model
        .update(Action::TerminalEvent(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))));
    assert_eq!(model.view, View::Conversation);
}

#[test]
fn delayed_detach_acknowledgement_cannot_remove_a_new_terminal() {
    use peritus_app_protocol::{
        AppResponseEnvelope, AppResponsePayload, OperationAcknowledgement, TerminalExit,
        TerminalExitDisposition,
    };
    let mut model = attached_model();
    model.terminal.as_mut().unwrap().set_capture_input(false);
    let detached = terminal_request(model.update(Action::TerminalEvent(Event::Key(
        KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE),
    ))));
    let terminal = model.terminal.as_mut().unwrap();
    terminal
        .accept_exit(TerminalExit::new(terminal.binding(), 0, 0, TerminalExitDisposition::Code(0)))
        .unwrap();
    model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('a'),
        KeyModifiers::NONE,
    ))));
    model.update(Action::TerminalEvent(Event::Paste("a".repeat(32))));
    let attached = terminal_request(model.update(Action::TerminalEvent(Event::Key(
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
    ))));
    let AppRequestPayload::AttachTerminal(binding) = attached.payload() else {
        panic!("attach request expected")
    };
    model.update(Action::Message(AppMessage::Response(AppResponseEnvelope::new(
        attached.context(),
        attached.request_id(),
        attached.correlation_id(),
        AppResponsePayload::TerminalAttached(*binding),
    ))));
    model.update(Action::Message(AppMessage::Response(AppResponseEnvelope::new(
        detached.context(),
        detached.request_id(),
        detached.correlation_id(),
        AppResponsePayload::Acknowledged(OperationAcknowledgement::new(detached.request_id())),
    ))));
    assert_eq!(model.terminal.as_ref().unwrap().binding(), *binding);
}

#[test]
fn unsolicited_terminal_attachment_cannot_take_over_keyboard_capture() {
    use peritus_app_protocol::{AppResponseEnvelope, AppResponsePayload, CorrelationId};
    let mut model = attached_model();
    let original = model.terminal.as_ref().unwrap().binding();
    let other = TerminalBinding::new(
        TerminalAttachmentId::new([51; 16]).unwrap(),
        ProcessId::new([52; 16]).unwrap(),
        RequestId::new([53; 16]).unwrap(),
    );
    model.view = View::Approvals;
    model.update(Action::Message(AppMessage::Response(AppResponseEnvelope::new(
        context(),
        other.originating_request_id(),
        CorrelationId::new([54; 16]).unwrap(),
        AppResponsePayload::TerminalAttached(other),
    ))));
    assert_eq!(model.terminal.as_ref().unwrap().binding(), original);
    assert_eq!(model.view, View::Approvals);
}

fn terminal_request(effects: Vec<Effect>) -> peritus_app_protocol::AppRequestEnvelope {
    effects
        .into_iter()
        .find_map(|effect| match effect {
            Effect::Send(AppMessage::Request(request)) => Some(request),
            _ => None,
        })
        .expect("terminal request")
}

fn attached_model() -> AppModel {
    let launch = ProductLaunchContext::new(
        WorkspaceId::new([4; 16]).expect("workspace"),
        "fixture".to_owned(),
        vec![ProductProviderOption::new(
            ProviderProfileId::new([5; 16]).expect("provider"),
            "Fixture",
        )],
        Some(0),
    )
    .expect("launch");
    let mut model = AppModel::with_product([6; 32], Some(launch));
    model.context = Some(context());
    let binding = TerminalBinding::new(
        TerminalAttachmentId::new([7; 16]).expect("attachment"),
        ProcessId::new([8; 16]).expect("process"),
        RequestId::new([9; 16]).expect("request"),
    );
    model.terminal = Some(crate::terminal::TerminalSession::new(binding, 8192).expect("terminal"));
    model.view = View::Terminal;
    model
}
