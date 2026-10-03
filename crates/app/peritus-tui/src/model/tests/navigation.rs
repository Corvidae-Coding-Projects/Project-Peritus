use super::*;
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn standalone_numbered_tabs_match_keyboard_cycle_and_preview_explains_its_requirement() {
    let mut model = AppModel::new([45; 32]);
    for (number, view) in ('1'..='9').zip(View::ALL) {
        model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
            KeyCode::Char(number),
            KeyModifiers::NONE,
        ))));
        assert_eq!(model.view, view);
        model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
            KeyCode::BackTab,
            KeyModifiers::SHIFT,
        ))));
        model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
            KeyCode::Tab,
            KeyModifiers::NONE,
        ))));
        assert_eq!(model.view, view);
    }
    model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
        KeyCode::Char('8'),
        KeyModifiers::NONE,
    ))));
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    let frame = terminal.draw(|frame| crate::render::draw(frame, &model)).unwrap();
    let text = frame.buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect::<String>();
    assert!(text.contains("Preview requires a project workspace"));
    assert!(!text.contains("No matching live events"));
}
