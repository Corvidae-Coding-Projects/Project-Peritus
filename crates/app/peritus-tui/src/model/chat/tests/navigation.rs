use super::*;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Modifier};

fn modified(model: &mut AppModel, code: KeyCode, modifiers: KeyModifiers) {
    let _ = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(code, modifiers))));
}

fn draft(text: &str) -> AppModel {
    let mut model = model();
    model.chat.buffer = text.to_owned();
    model.chat.cursor = text.len();
    model
}

fn mouse(
    model: &mut AppModel,
    kind: MouseEventKind,
    column: u16,
    row: u16,
    modifiers: KeyModifiers,
) {
    let _ = model.update(Action::TerminalEvent(Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers,
    })));
}

fn render(model: &mut AppModel, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
    terminal
        .draw(|frame| {
            model.chat.viewport = Some(frame.area());
            crate::render::draw(frame, model);
        })
        .expect("draw");
    terminal
}

#[test]
fn word_navigation_handles_unicode_punctuation_whitespace_and_boundaries() {
    let mut model = draft("hello,  λ界\nnext_word");
    for expected in ["hello,  λ界\n".len(), 8, 5, 0, 0] {
        modified(&mut model, KeyCode::Left, KeyModifiers::CONTROL);
        assert_eq!(model.chat.cursor, expected);
    }
    for expected in [5, 8, "hello,  λ界\n".len(), model.chat.buffer.len(), model.chat.buffer.len()]
    {
        modified(&mut model, KeyCode::Right, KeyModifiers::CONTROL);
        assert_eq!(model.chat.cursor, expected);
    }
}

#[test]
fn shift_selection_reverses_collapses_and_replaces_without_splitting_utf8() {
    let mut model = draft("a λ界 tail");
    modified(&mut model, KeyCode::Left, KeyModifiers::CONTROL | KeyModifiers::SHIFT);
    assert_eq!(model.chat.selection(), Some(8..12));
    modified(&mut model, KeyCode::Right, KeyModifiers::SHIFT);
    assert_eq!(model.chat.selection(), Some(9..12));
    modified(&mut model, KeyCode::Left, KeyModifiers::NONE);
    assert_eq!(model.chat.cursor, 9);
    assert_eq!(model.chat.selection(), None);
    modified(&mut model, KeyCode::Home, KeyModifiers::SHIFT);
    modified(&mut model, KeyCode::Char('界'), KeyModifiers::NONE);
    assert_eq!(model.chat.buffer, "界ail");
    assert_eq!(model.chat.cursor, 3);
    modified(&mut model, KeyCode::End, KeyModifiers::SHIFT);
    modified(&mut model, KeyCode::Delete, KeyModifiers::NONE);
    assert_eq!(model.chat.buffer, "界");
    modified(&mut model, KeyCode::Home, KeyModifiers::SHIFT);
    modified(&mut model, KeyCode::Backspace, KeyModifiers::NONE);
    assert_eq!(model.chat.buffer, "");
    assert_eq!(model.chat.cursor, 0);
}

#[test]
fn replacement_respects_byte_limit_and_rejected_paste_retains_selection() {
    let mut model = draft(&"a".repeat(peritus_app_protocol::MAX_PRODUCT_TASK_BYTES));
    modified(&mut model, KeyCode::Home, KeyModifiers::SHIFT);
    let selection = model.chat.selection();
    model.paste_chat(&"b".repeat(peritus_app_protocol::MAX_PRODUCT_TASK_BYTES + 1));
    assert_eq!(model.chat.selection(), selection);
    assert_eq!(model.chat.buffer.as_bytes()[0], b'a');
    model.paste_chat("λ界");
    assert_eq!(model.chat.buffer, "λ界");
    assert_eq!(model.chat.cursor, 5);
    assert_eq!(model.chat.selection(), None);
    model.chat.buffer = "a".repeat(peritus_app_protocol::MAX_PRODUCT_TASK_BYTES - 1);
    model.chat.cursor = model.chat.buffer.len();
    modified(&mut model, KeyCode::Char('λ'), KeyModifiers::NONE);
    assert_eq!(model.chat.buffer.len(), peritus_app_protocol::MAX_PRODUCT_TASK_BYTES - 1);
}

#[test]
fn paste_over_backwards_command_selection_cannot_acquire_keyboard_intent() {
    let mut model = draft("/help argument");
    modified(&mut model, KeyCode::Home, KeyModifiers::NONE);
    modified(&mut model, KeyCode::End, KeyModifiers::SHIFT);
    model.paste_chat_event("/quit");
    assert!(model.chat.pasted_command);
    assert!(key(&mut model, KeyCode::Enter).is_empty());
    assert!(!model.quitting);
    assert_eq!(model.chat.buffer, "/quit");
}

#[test]
fn escape_completion_submission_and_restore_clear_selection() {
    let mut model = draft("/he");
    modified(&mut model, KeyCode::Home, KeyModifiers::SHIFT);
    modified(&mut model, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(model.chat.selection(), None);
    modified(&mut model, KeyCode::End, KeyModifiers::SHIFT);
    modified(&mut model, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(model.chat.selection_anchor, None);
    model.restore_chat_draft("question");
    modified(&mut model, KeyCode::Home, KeyModifiers::SHIFT);
    assert!(!key(&mut model, KeyCode::Enter).is_empty());
    assert_eq!(model.chat.selection_anchor, None);
    model.chat.mouse_anchor = Some(1);
    model.restore_chat_draft("restored");
    assert_eq!(model.chat.selection_anchor, None);
    assert_eq!(model.chat.mouse_anchor, None);
}

#[test]
fn clicks_use_rendered_cursor_cells_and_drag_highlights_selected_text() {
    let mut model = draft("ab界cd\nλ tail");
    let mut terminal = render(&mut model, 40, 20);
    let end = terminal.get_cursor_position().expect("cursor");
    mouse(&mut model, MouseEventKind::Down(MouseButton::Left), 3, end.y - 1, KeyModifiers::NONE);
    assert_eq!(model.chat.cursor, 2);
    mouse(&mut model, MouseEventKind::Drag(MouseButton::Left), 5, end.y - 1, KeyModifiers::NONE);
    assert_eq!(model.chat.selection(), Some(2..5));
    mouse(&mut model, MouseEventKind::Up(MouseButton::Left), 5, end.y - 1, KeyModifiers::NONE);
    let terminal = render(&mut model, 40, 20);
    assert!(terminal.backend().buffer()[(3, end.y - 1)].modifier.contains(Modifier::REVERSED));
    modified(&mut model, KeyCode::Char('X'), KeyModifiers::NONE);
    assert_eq!(model.chat.buffer, "abXcd\nλ tail");
}

#[test]
fn clicks_follow_scrolled_wrapped_content_and_resize_invalidates_old_geometry() {
    let mut model = draft("0123456789\n".repeat(10).as_str());
    let mut terminal = render(&mut model, 12, 20);
    let end = terminal.get_cursor_position().expect("cursor");
    mouse(&mut model, MouseEventKind::Down(MouseButton::Left), 4, end.y - 1, KeyModifiers::NONE);
    assert_eq!(model.chat.cursor, 9 * 11 + 3);
    let _ = model.update(Action::TerminalEvent(Event::Resize(40, 24)));
    mouse(&mut model, MouseEventKind::Down(MouseButton::Left), 2, end.y, KeyModifiers::NONE);
    assert_eq!(model.chat.cursor, 102);
    let mut terminal = render(&mut model, 40, 24);
    let cursor = terminal.get_cursor_position().expect("cursor");
    mouse(
        &mut model,
        MouseEventKind::Down(MouseButton::Left),
        cursor.x + 1,
        cursor.y,
        KeyModifiers::SHIFT,
    );
    assert_eq!(model.chat.selection(), Some(102..103));
}

#[test]
fn borders_other_views_and_panels_do_not_capture_composer_clicks() {
    let mut model = draft("draft");
    let mut terminal = render(&mut model, 40, 20);
    let cursor = terminal.get_cursor_position().expect("cursor");
    mouse(&mut model, MouseEventKind::Down(MouseButton::Left), 0, cursor.y, KeyModifiers::NONE);
    assert_eq!(model.chat.cursor, 5);
    model.view = View::Runs;
    mouse(&mut model, MouseEventKind::Down(MouseButton::Left), 1, cursor.y, KeyModifiers::NONE);
    assert_eq!(model.chat.cursor, 5);
    model.view = View::Conversation;
    model.chat.workbench.open = true;
    mouse(&mut model, MouseEventKind::Down(MouseButton::Left), 1, cursor.y, KeyModifiers::NONE);
    assert_eq!(model.chat.cursor, 5);
    model.chat.viewport = Some(Rect::new(0, 0, 1, 1));
    model.chat.workbench.open = false;
    mouse(&mut model, MouseEventKind::Down(MouseButton::Left), 0, 0, KeyModifiers::NONE);
    assert_eq!(model.chat.cursor, 5);
}
