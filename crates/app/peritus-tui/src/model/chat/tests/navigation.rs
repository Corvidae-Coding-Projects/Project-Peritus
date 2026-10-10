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

#[test]
fn inactive_composer_is_not_edited_by_paste_into_an_inspection_or_picker() {
    for overlay in 0..3 {
        let mut model = draft("unsent draft");
        model.chat.cursor = 3;
        model.chat.selection_anchor = Some(0);
        match overlay {
            0 => model.chat.workbench.open = true,
            1 => model.chat.show_model_picker(),
            _ => model.chat.show_effort_picker(),
        }
        let effects = model.update(Action::TerminalEvent(Event::Paste("accidental paste".into())));
        assert!(effects.is_empty());
        assert_eq!(model.chat.buffer, "unsent draft", "overlay {overlay}");
        assert_eq!(model.chat.cursor, 3);
        assert_eq!(model.chat.selection_anchor, Some(0));
    }
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
fn inspection_pages_scroll_and_home_returns_to_the_start() {
    use peritus_app_protocol::{ProductRunPhase, ProductRunSnapshot};
    let mut model = model();
    model.view = View::Review;
    key(&mut model, KeyCode::PageDown);
    assert_eq!(model.product.as_ref().expect("product").inspection_scroll, 0);
    let product = model.product.as_ref().expect("product");
    let run = ProductRunSnapshot::new(
        RunId::new([49; 16]).expect("run"),
        product.launch.workspace_id(),
        model.chat_providers().expect("providers"),
        ProductRunPhase::Complete,
        1,
        "inspect long result".to_owned(),
        "complete".to_owned(),
        "diff line\n".repeat(100),
        String::new(),
        "review line\n".repeat(100),
        String::new(),
        crate::test_support::run_operation(
            RunId::new([49; 16]).expect("run"),
            ProductRunPhase::Complete,
        ),
    )
    .expect("run");
    model.accept_product_run(run);
    for view in [View::Diff, View::Review] {
        model.view = view;
        let _ = key(&mut model, KeyCode::PageDown);
        assert_eq!(model.product.as_ref().expect("product").inspection_scroll, 12);
        let _ = key(&mut model, KeyCode::PageUp);
        assert_eq!(model.product.as_ref().expect("product").inspection_scroll, 0);
        let _ = key(&mut model, KeyCode::PageDown);
        let _ = key(&mut model, KeyCode::Home);
        assert_eq!(model.product.as_ref().expect("product").inspection_scroll, 0);
        for _ in 0..100 {
            key(&mut model, KeyCode::PageDown);
        }
        let bottom = model.product.as_ref().expect("product").inspection_scroll;
        key(&mut model, KeyCode::PageUp);
        assert_eq!(model.product.as_ref().expect("product").inspection_scroll, bottom - 12);
        key(&mut model, KeyCode::Home);
    }
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
fn output_selection_owns_copy_keys_without_cancelling_or_editing() {
    let mut model = draft("unsent draft");
    assert!(key(&mut model, KeyCode::F(2)).is_empty());
    assert!(model.chat.selecting_output());
    modified(&mut model, KeyCode::Char('c'), KeyModifiers::CONTROL | KeyModifiers::SHIFT);
    modified(&mut model, KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert!(!model.quitting);
    assert_eq!(model.chat.buffer, "unsent draft");
    assert!(key(&mut model, KeyCode::Esc).is_empty());
    assert!(!model.chat.selecting_output());
    let _ = key(&mut model, KeyCode::Char('!'));
    assert_eq!(model.chat.buffer, "unsent draft!");
}

#[test]
fn paste_after_output_copy_resumes_live_output_and_edits_the_draft() {
    let mut model = draft("unsent draft");
    assert!(key(&mut model, KeyCode::F(2)).is_empty());
    assert!(model.chat.selecting_output());

    let effects = model.update(Action::TerminalEvent(Event::Paste(" pasted text\n".to_owned())));

    assert!(effects.is_empty());
    assert!(!model.chat.selecting_output());
    assert_eq!(model.chat.buffer, "unsent draft pasted text\n");
    assert_eq!(model.chat.cursor, model.chat.buffer.len());
}

#[test]
fn right_click_opens_terminal_selection_and_mouse_paste_returns_to_the_draft() {
    let mut model = draft("mouse draft");
    mouse(&mut model, MouseEventKind::Down(MouseButton::Right), 12, 4, KeyModifiers::NONE);
    assert!(model.chat.selecting_output());

    let effects = model.update(Action::TerminalEvent(Event::Paste(" pasted".to_owned())));

    assert!(effects.is_empty());
    assert!(!model.chat.selecting_output());
    assert_eq!(model.chat.buffer, "mouse draft pasted");
}

#[test]
fn mouse_wheel_scrolls_the_transcript_in_bounded_steps() {
    let mut model = draft("unsent draft");
    model.chat.mouse_anchor = Some(4);

    mouse(&mut model, MouseEventKind::ScrollUp, 10, 4, KeyModifiers::NONE);
    assert_eq!(model.chat.scroll, 3);
    assert_eq!(model.chat.mouse_anchor, None);
    mouse(&mut model, MouseEventKind::ScrollUp, 10, 4, KeyModifiers::CONTROL);
    assert_eq!(model.chat.scroll, 6);
    mouse(&mut model, MouseEventKind::ScrollDown, 10, 4, KeyModifiers::NONE);
    assert_eq!(model.chat.scroll, 3);
    mouse(&mut model, MouseEventKind::ScrollDown, 10, 4, KeyModifiers::NONE);
    mouse(&mut model, MouseEventKind::ScrollDown, 10, 4, KeyModifiers::NONE);

    assert_eq!(model.chat.scroll, 0);
    assert_eq!(model.chat.buffer, "unsent draft");
    assert_eq!(model.chat.cursor, model.chat.buffer.len());
}

#[test]
fn mouse_wheel_does_not_scroll_behind_chat_overlays_or_other_views() {
    for overlay in 0..4 {
        let mut model = draft("unsent draft");
        model.chat.scroll = 7;
        match overlay {
            0 => model.chat.workbench.open = true,
            1 => model.chat.show_model_picker(),
            2 => model.chat.show_effort_picker(),
            _ => {
                let _ = key(&mut model, KeyCode::F(2));
                assert!(model.chat.selecting_output());
            }
        }
        mouse(&mut model, MouseEventKind::ScrollUp, 10, 4, KeyModifiers::NONE);
        assert_eq!(model.chat.scroll, 7, "overlay {overlay}");
    }

    let mut model = draft("unsent draft");
    model.chat.scroll = 7;
    model.view = View::Runs;
    mouse(&mut model, MouseEventKind::ScrollUp, 10, 4, KeyModifiers::NONE);
    assert_eq!(model.chat.scroll, 7);
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
fn replacement_accepts_large_paste_and_keeps_unicode_boundaries() {
    let mut model = draft(&"a".repeat(peritus_app_protocol::MAX_PRODUCT_TASK_BYTES));
    modified(&mut model, KeyCode::Home, KeyModifiers::SHIFT);
    let large = "b".repeat(peritus_app_protocol::MAX_PRODUCT_TASK_BYTES + 1);
    assert!(model.paste_chat(&large));
    assert_eq!(model.chat.buffer, large);
    assert_eq!(model.chat.selection(), None);
    modified(&mut model, KeyCode::Home, KeyModifiers::SHIFT);
    model.paste_chat("λ界");
    assert_eq!(model.chat.buffer, "λ界");
    assert_eq!(model.chat.cursor, 5);
    model.chat.buffer = "a".repeat(peritus_app_protocol::MAX_PRODUCT_TASK_BYTES - 1);
    model.chat.cursor = model.chat.buffer.len();
    modified(&mut model, KeyCode::Char('λ'), KeyModifiers::NONE);
    assert_eq!(model.chat.buffer.len(), peritus_app_protocol::MAX_PRODUCT_TASK_BYTES + 1);
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
fn escape_completion_and_submission_clear_selection() {
    let mut model = draft("/he");
    modified(&mut model, KeyCode::Home, KeyModifiers::SHIFT);
    modified(&mut model, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(model.chat.selection(), None);
    modified(&mut model, KeyCode::End, KeyModifiers::SHIFT);
    modified(&mut model, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(model.chat.selection_anchor, None);
    model.chat.buffer = "question".to_owned();
    model.chat.cursor = model.chat.buffer.len();
    modified(&mut model, KeyCode::Home, KeyModifiers::SHIFT);
    enable_durable_chat(&mut model);
    assert!(!key(&mut model, KeyCode::Enter).is_empty());
    assert_eq!(model.chat.selection_anchor, None);
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

#[test]
fn vertical_arrows_follow_wrapped_unicode_rows_and_shift_selects() {
    let mut model = draft("ab界cdλz");
    let _ = render(&mut model, 6, 20);
    modified(&mut model, KeyCode::Up, KeyModifiers::SHIFT);
    assert_eq!(model.chat.cursor, "ab界".len());
    assert_eq!(model.chat.selection(), Some("ab界".len()..model.chat.buffer.len()));
    modified(&mut model, KeyCode::Up, KeyModifiers::SHIFT);
    assert_eq!(model.chat.cursor, 0);
    assert_eq!(model.chat.selection(), Some(0..model.chat.buffer.len()));
    modified(&mut model, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(model.chat.selection(), None);
    assert_eq!(model.chat.cursor, "ab界".len());
    modified(&mut model, KeyCode::Char('!'), KeyModifiers::NONE);
    assert_eq!(model.chat.buffer, "ab界!cdλz");
}
