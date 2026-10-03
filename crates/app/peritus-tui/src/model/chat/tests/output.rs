//! Mouse-only copy follows the rendered public transcript and never controls daemon work.

use super::*;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use peritus_app_protocol::{
    ProductActivity, ProductActivityKind, ProductRunPhase, ProductRunSnapshot,
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    layout::{Position, Rect},
    style::Modifier,
};

fn conversation(text: &str) -> AppModel {
    let mut model = model();
    let run = RunId::new([83; 16]).unwrap();
    model.chat.run_id = Some(run);
    let phase = ProductRunPhase::Writing;
    model.chat.snapshot = Some(
        ProductInteractionSnapshot::new(
            ProductRunSnapshot::new(
                run,
                model.product.as_ref().unwrap().launch.workspace_id(),
                model.chat_providers().unwrap(),
                phase,
                1,
                "copy fixture".to_owned(),
                "Working".to_owned(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                crate::test_support::run_operation(run, phase),
            )
            .unwrap(),
            ProductInteractionMode::Chat,
            ProductRoleModels::default(),
            1,
            1,
            vec![
                ProductActivity::new(
                    1,
                    ProductActivityKind::Assistant,
                    text.to_owned(),
                    "Hidden implementation detail".to_owned(),
                )
                .unwrap(),
            ],
            None,
        )
        .unwrap(),
    );
    model.chat.buffer = "unsent draft".to_owned();
    model.chat.cursor = model.chat.buffer.len();
    let _ = model.update(Action::Tick(std::time::Instant::now()));
    model
}

fn render(model: &mut AppModel, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            model.chat.viewport = Some(frame.area());
            crate::render::draw(frame, model);
        })
        .unwrap();
    terminal
}

fn mouse(model: &mut AppModel, kind: MouseEventKind, x: u16, y: u16) -> Vec<Effect> {
    model.update(Action::TerminalEvent(Event::Mouse(MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    })))
}

fn select(model: &mut AppModel, start: Position, end: Position) {
    assert!(mouse(model, MouseEventKind::Down(MouseButton::Left), start.x, start.y).is_empty());
    assert!(mouse(model, MouseEventKind::Drag(MouseButton::Left), end.x, end.y).is_empty());
    assert!(mouse(model, MouseEventKind::Up(MouseButton::Left), end.x, end.y).is_empty());
}

fn selected(model: &AppModel) -> &str {
    model.chat.output_selection.as_ref().unwrap().selected_text().unwrap()
}

#[test]
fn mouse_drag_right_click_and_copy_click_preserve_unicode_draft_and_live_work() {
    let mut model = conversation("a λ界 tail\nsecond line");
    let _ = render(&mut model, 60, 28);
    let area = crate::render::transcript_area(&model, model.chat.viewport.unwrap());
    select(&mut model, Position::new(2, area.y + 1), Position::new(5, area.y + 1));
    assert_eq!(selected(&model), "λ界");
    assert!(!model.chat.selecting_output(), "application selection retains mouse capture");
    let terminal = render(&mut model, 60, 28);
    assert!(terminal.backend().buffer()[(2, area.y + 1)].modifier.contains(Modifier::REVERSED));
    let new_text = "Changed while selection is open";
    let updated = conversation(new_text).chat.snapshot.unwrap();
    model.accept_chat(updated);
    assert_eq!(selected(&model), "λ界");
    let terminal = render(&mut model, 60, 28);
    let screen: String =
        terminal.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect();
    assert_eq!(terminal.backend().buffer()[(0, area.y + 1)].symbol(), "a");
    assert_eq!(terminal.backend().buffer()[(2, area.y + 1)].symbol(), "λ");
    assert_eq!(terminal.backend().buffer()[(3, area.y + 1)].symbol(), "界");
    assert!(screen.contains(" tail"));
    assert!(!screen.contains(new_text));
    assert!(mouse(&mut model, MouseEventKind::Down(MouseButton::Right), 59, 27).is_empty());
    let menu = model.chat.output_selection.as_ref().unwrap().menu.unwrap();
    assert!(Rect::new(0, 0, 60, 28).contains(Position::new(menu.right() - 1, menu.bottom() - 1)));
    let terminal = render(&mut model, 60, 28);
    assert_eq!(terminal.backend().buffer()[(menu.x + 2, menu.y + 1)].symbol(), "C");
    let effects =
        mouse(&mut model, MouseEventKind::Down(MouseButton::Left), menu.x + 2, menu.y + 1);
    assert!(matches!(effects.as_slice(), [Effect::CopyText { text, .. }] if text == "λ界"));
    assert!(
        mouse(&mut model, MouseEventKind::Down(MouseButton::Left), menu.x + 2, menu.y + 1)
            .is_empty()
    );
    assert_eq!(selected(&model), "λ界");
    assert_eq!(model.chat.buffer, "unsent draft");
    assert!(!model.quitting);
    assert!(
        model.pending.values().all(|pending| !matches!(pending, PendingRequest::ProductControl))
    );
    let operation = model.chat.output_selection.as_ref().unwrap().copy_operation.unwrap();
    let _ = model.update(Action::ClipboardWritten {
        operation,
        result: Ok(crate::action::ClipboardDestination::Desktop),
    });
    assert!(model.chat.output_selection.is_none());
    let terminal = render(&mut model, 60, 28);
    let screen: String =
        terminal.backend().buffer().content().iter().map(ratatui::buffer::Cell::symbol).collect();
    assert!(screen.contains(new_text));
}

#[test]
fn accented_letters_and_joined_emoji_are_selected_as_their_rendered_glyphs() {
    let mut model = conversation("a e\u{301} 👩\u{200d}💻 b");
    let _ = render(&mut model, 12, 30);
    let area = crate::render::transcript_area(&model, model.chat.viewport.unwrap());
    select(&mut model, Position::new(2, area.y + 1), Position::new(6, area.y + 1));
    assert_eq!(selected(&model), "e\u{301} 👩\u{200d}💻");
    let terminal = render(&mut model, 12, 30);
    assert_eq!(terminal.backend().buffer()[(2, area.y + 1)].symbol(), "e\u{301}");
    assert_eq!(terminal.backend().buffer()[(4, area.y + 1)].symbol(), "👩\u{200d}💻");
    assert!(terminal.backend().buffer()[(4, area.y + 1)].modifier.contains(Modifier::REVERSED));
}

#[test]
fn reverse_multiline_copy_preserves_hard_newlines_but_joins_screen_wraps() {
    let text = "0123456789abcdef\nλ界 last";
    let mut model = conversation(text);
    let _ = render(&mut model, 12, 30);
    let area = crate::render::transcript_area(&model, model.chat.viewport.unwrap());
    select(&mut model, Position::new(8, area.y + 3), Position::new(0, area.y + 1));
    assert_eq!(selected(&model), text);
    let effects = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    ))));
    assert!(matches!(effects.as_slice(), [Effect::CopyText { text: value, .. }] if value == text));
    let repeated = model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    ))));
    assert!(repeated.is_empty());
    assert_eq!(selected(&model), text);
    assert!(!model.chat.interrupt_requested);
    assert!(!model.quitting);
    assert_eq!(model.chat.buffer, "unsent draft");
    let operation = model.chat.output_selection.as_ref().unwrap().copy_operation.unwrap();
    let _ =
        model.update(Action::ClipboardWritten { operation, result: Err("broken pipe".to_owned()) });
    assert_eq!(selected(&model), text);
    assert!(model.notice.as_ref().unwrap().text.contains("Copy failed"));
    assert!(key(&mut model, KeyCode::Esc).is_empty());
    assert!(model.chat.output_selection.is_none());
    assert!(!model.chat.interrupt_requested);
}

#[test]
fn a_delayed_clipboard_receipt_does_not_clear_a_new_selection() {
    let mut model = conversation("first second");
    let _ = render(&mut model, 60, 28);
    let area = crate::render::transcript_area(&model, model.chat.viewport.unwrap());
    select(&mut model, Position::new(0, area.y + 1), Position::new(5, area.y + 1));
    let effects = model.copy_selected_output();
    let [Effect::CopyText { operation, text }] = effects.as_slice() else {
        panic!("clipboard copy")
    };
    assert_eq!(text, "first");
    let operation = *operation;
    select(&mut model, Position::new(6, area.y + 1), Position::new(12, area.y + 1));
    let _ = model.update(Action::ClipboardWritten {
        operation,
        result: Ok(crate::action::ClipboardDestination::Desktop),
    });
    assert_eq!(selected(&model), "second");
    assert_eq!(model.chat.buffer, "unsent draft");
}

#[test]
fn selection_is_discarded_by_resize_scroll_paste_composer_click_and_target_changes() {
    for action in 0..8 {
        let mut model = conversation("copy this output");
        let mut terminal = render(&mut model, 60, 28);
        let cursor = terminal.get_cursor_position().unwrap();
        let area = crate::render::transcript_area(&model, model.chat.viewport.unwrap());
        select(&mut model, Position::new(0, area.y + 1), Position::new(4, area.y + 1));
        assert_eq!(selected(&model), "copy");
        match action {
            0 => {
                let _ = model.update(Action::TerminalEvent(Event::Resize(80, 30)));
            }
            1 => {
                let _ = mouse(&mut model, MouseEventKind::ScrollUp, 1, area.y);
            }
            2 => {
                let _ = model.update(Action::TerminalEvent(Event::Paste(" pasted".to_owned())));
            }
            3 => {
                let _ =
                    mouse(&mut model, MouseEventKind::Down(MouseButton::Left), cursor.x, cursor.y);
            }
            4 => {
                let _ = key(&mut model, KeyCode::F(2));
            }
            5 => {
                model.chat.run_id = Some(RunId::new([84; 16]).unwrap());
                model.chat.workbench.open = true;
                let _ = model.update(Action::Tick(std::time::Instant::now()));
            }
            6 => {
                model.view = View::Runs;
                let _ = model.update(Action::Tick(std::time::Instant::now()));
            }
            _ => {
                let _ = key(&mut model, KeyCode::Char('!'));
            }
        }
        assert!(model.chat.output_selection.is_none(), "action {action}");
        assert!(!model.quitting);
    }
}

#[test]
fn collapsed_tool_copy_contains_only_the_sanitized_visible_projection() {
    let mut model = conversation("unused");
    let mut snapshot = model.chat.snapshot.clone().unwrap();
    let activity = ProductActivity::new(1, ProductActivityKind::Tool, "Ran a command".to_owned(),
        "line1\nline2\nline3\nline4 hidden\nline5\nline6\nline7\nline8\n\x1b[31mvisible tail\x1b[0m".to_owned()).unwrap();
    snapshot = ProductInteractionSnapshot::new(
        snapshot.snapshot().clone(),
        snapshot.mode(),
        snapshot.models().clone(),
        1,
        1,
        vec![activity],
        None,
    )
    .unwrap();
    model.chat.snapshot = Some(snapshot);
    let _ = render(&mut model, 80, 30);
    let area = crate::render::transcript_area(&model, model.chat.viewport.unwrap());
    select(&mut model, Position::new(0, area.y), Position::new(79, area.bottom() - 1));
    let text = selected(&model);
    assert!(text.contains("more lines"));
    assert!(text.contains("visible tail"));
    assert!(!text.contains("hidden"));
    assert!(!text.contains('\x1b'));
}
