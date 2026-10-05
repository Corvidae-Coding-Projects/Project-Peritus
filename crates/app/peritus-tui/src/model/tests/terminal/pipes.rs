//! Pipe input stays local until Enter and is retired only by its exact accepted request.

use super::*;
use peritus_app_protocol::{AppResponseEnvelope, AppResponsePayload, OperationAcknowledgement};

fn key(model: &mut AppModel, code: KeyCode) -> Vec<Effect> {
    model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(code, KeyModifiers::NONE))))
}

#[test]
fn pipe_input_edits_unicode_and_retains_rejected_lines() {
    let mut model = attached_model();
    model.terminal.as_mut().unwrap().use_pipes();
    assert!(model.update(Action::TerminalEvent(Event::Paste("Aλx".into()))).is_empty());
    assert!(key(&mut model, KeyCode::Left).is_empty());
    assert!(key(&mut model, KeyCode::Backspace).is_empty());
    assert_eq!(model.terminal.as_ref().unwrap().line_input().unwrap().0, "Ax");
    assert!(model.update(Action::TerminalEvent(Event::Resize(100, 30))).is_empty());
    let sent = terminal_request(key(&mut model, KeyCode::Enter));
    assert!(
        matches!(sent.payload(), AppRequestPayload::TerminalInput(input) if input.bytes() == b"Ax\n")
    );
    assert!(key(&mut model, KeyCode::Enter).is_empty(), "no duplicate send while pending");
    model.update(Action::Message(AppMessage::Response(AppResponseEnvelope::new(
        sent.context(),
        sent.request_id(),
        sent.correlation_id(),
        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
            peritus_app_protocol::AppErrorCode::Backpressure,
            None,
        )),
    ))));
    assert_eq!(model.terminal.as_ref().unwrap().line_input(), Some(("Ax", 1, false)));
    let retried = terminal_request(key(&mut model, KeyCode::Enter));
    model.update(Action::Message(AppMessage::Response(AppResponseEnvelope::new(
        retried.context(),
        retried.request_id(),
        retried.correlation_id(),
        AppResponsePayload::Acknowledged(OperationAcknowledgement::new(retried.request_id())),
    ))));
    assert_eq!(model.terminal.as_ref().unwrap().line_input(), Some(("", 0, false)));
}

#[test]
fn stale_line_ack_cannot_clear_a_new_attachment_draft() {
    let mut model = attached_model();
    model.terminal.as_mut().unwrap().use_pipes();
    model.update(Action::TerminalEvent(Event::Paste("old".into())));
    let sent = terminal_request(key(&mut model, KeyCode::Enter));
    let binding = TerminalBinding::new(
        TerminalAttachmentId::new([61; 16]).unwrap(),
        ProcessId::new([62; 16]).unwrap(),
        RequestId::new([63; 16]).unwrap(),
    );
    let mut terminal = crate::terminal::TerminalSession::new(binding, 8192).unwrap();
    terminal.use_pipes();
    terminal.paste_bytes("new");
    model.terminal = Some(terminal);
    model.update(Action::Message(AppMessage::Response(AppResponseEnvelope::new(
        sent.context(),
        sent.request_id(),
        sent.correlation_id(),
        AppResponsePayload::Acknowledged(OperationAcknowledgement::new(sent.request_id())),
    ))));
    assert_eq!(model.terminal.as_ref().unwrap().line_input(), Some(("new", 3, false)));
}

#[test]
fn negotiated_pipe_reply_selects_line_input_before_any_output_or_resize() {
    let mut model = attached_model();
    model.terminal = None;
    let request = terminal_request(model.attach_terminal(&"4a".repeat(16)));
    let AppRequestPayload::AttachTerminal(binding) = request.payload() else { panic!("attach") };
    let effects = model.update(Action::Message(AppMessage::Response(AppResponseEnvelope::new(
        request.context(),
        request.request_id(),
        request.correlation_id(),
        AppResponsePayload::TerminalPipeAttached(*binding),
    ))));
    assert!(effects.is_empty(), "pipe mode must not send the initial PTY resize");
    assert!(model.terminal.as_ref().unwrap().uses_pipes());
    assert!(model.update(Action::TerminalEvent(Event::Paste("Ada".into()))).is_empty());
    let request = terminal_request(key(&mut model, KeyCode::Enter));
    assert!(
        matches!(request.payload(), AppRequestPayload::TerminalInput(input) if input.bytes() == b"Ada\n")
    );
}

#[test]
fn pipe_interrupt_cancels_the_child_without_quitting_chat() {
    let mut model = attached_model();
    model.terminal.as_mut().unwrap().use_pipes();
    let request = terminal_request(model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    )))));
    assert!(matches!(request.payload(), AppRequestPayload::CancelTerminal(_)));
    assert!(!model.quitting);
}

#[test]
fn unconfirmed_pipe_input_keeps_chat_connected_and_draft_visible() {
    let mut model = attached_model();
    model.terminal.as_mut().unwrap().use_pipes();
    model.update(Action::TerminalEvent(Event::Paste("Ada".into())));
    let _sent = terminal_request(key(&mut model, KeyCode::Enter));
    let context = model.context;
    model.tick_count += 121;
    let effects = model.update(Action::Tick(std::time::Instant::now()));
    assert!(!effects.iter().any(|effect| matches!(effect, Effect::Reconnect)));
    assert_eq!(model.context, context);
    assert_eq!(model.terminal.as_ref().unwrap().line_input(), Some(("Ada", 3, true)));
}

#[test]
fn pipe_draft_and_cursor_remain_visible_at_normal_and_tiny_sizes() {
    use ratatui::{Terminal, backend::TestBackend};
    let mut model = attached_model();
    model.terminal.as_mut().unwrap().use_pipes();
    model.update(Action::TerminalEvent(Event::Paste("Ada λ".into())));
    for (width, height) in [(80, 24), (32, 10), (8, 5), (2, 2), (1, 1)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| crate::render::draw(frame, &model)).unwrap();
        if width >= 32 {
            let text = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(ratatui::buffer::Cell::symbol)
                .collect::<String>();
            assert!(text.contains("Ada λ"), "draft invisible in {width}x{height}: {text}");
        }
    }
}
