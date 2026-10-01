//! Modal input rendering.

use super::{ACCENT, MUTED};
use crate::{
    input::composer,
    model::{Editor, EditorKind},
};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

pub(super) fn render_editor(frame: &mut Frame<'_>, editor: &Editor) {
    let area = composer::modal_area(frame.area());
    frame.render_widget(Clear, area);
    let (title, keys) = match &editor.kind {
        EditorKind::ReviewFeedback(target) => (
            format!(" {} · {} ", editor.title, target.anchor.path()),
            "Enter submit · Ctrl-F refresh · Ctrl-B rebind · Esc cancel",
        ),
        _ => (format!(" {} ", editor.title), "Enter submit · Ctrl-R reconnect · Esc cancel"),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(title)
        .title_alignment(Alignment::Center);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [hint, body, footer] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(1), Constraint::Length(1)])
            .areas(inner);
    frame.render_widget(
        Paragraph::new(editor.hint).style(Style::default().fg(MUTED)).wrap(Wrap { trim: false }),
        hint,
    );
    frame.render_widget(Paragraph::new(keys).style(Style::default().fg(MUTED)), footer);
    if body.width == 0 || body.height == 0 {
        return;
    }
    let draft = composer::layout(&editor.buffer, editor.cursor, None, usize::from(body.width));
    let rows = usize::from(body.height);
    let offset = draft.row.saturating_sub(rows - 1);
    let column = u16::try_from(draft.column).unwrap_or(u16::MAX).min(body.width - 1);
    let row = u16::try_from(draft.row - offset).unwrap_or(u16::MAX).min(body.height - 1);
    frame.render_widget(
        Paragraph::new(draft.lines.into_iter().skip(offset).take(rows).collect::<Vec<_>>())
            .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        body,
    );
    frame.set_cursor_position((body.x + column, body.y + row));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::EditorKind;
    use ratatui::{Terminal, backend::TestBackend};

    fn screen(text: &str) -> (ratatui::buffer::Buffer, ratatui::layout::Position) {
        let editor = Editor {
            kind: EditorKind::ProductTask,
            title: "Draft",
            hint: "Enter submits",
            buffer: text.to_owned(),
            cursor: text.len(),
            pasted_command: false,
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| render_editor(frame, &editor)).unwrap();
        (terminal.backend().buffer().clone(), terminal.get_cursor_position().unwrap())
    }

    #[test]
    fn multiline_modal_keeps_the_editing_tail_visible() {
        let (buffer, cursor) = screen(&format!("{}tail marker", "earlier line\n".repeat(30)));
        let text = buffer.content().iter().map(ratatui::buffer::Cell::symbol).collect::<String>();
        assert!(text.contains("tail marker"));
        assert!(cursor.x > 0 && cursor.y > 0 && cursor.x < 79 && cursor.y < 23);
    }

    #[test]
    fn modal_cursor_uses_terminal_cells_for_wide_and_utf8_characters() {
        let (buffer, cursor) = screen("ab界λ");
        let start = buffer
            .content()
            .windows(2)
            .position(|cells| cells[0].symbol() == "a" && cells[1].symbol() == "b")
            .unwrap();
        assert_eq!(usize::from(cursor.x), start % 80 + 5);
        assert_eq!(usize::from(cursor.y), start / 80);
    }

    #[test]
    fn recovered_modal_is_visible_over_the_conversation_view() {
        let mut model = crate::model::AppModel::new([61; 32]);
        model.view = crate::model::View::Conversation;
        model.editor = Some(Editor {
            kind: EditorKind::ProductTask,
            title: "Recovered task",
            hint: "Enter submits",
            buffer: "recover this exact task".to_owned(),
            cursor: 0,
            pasted_command: false,
        });
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|frame| super::super::draw(frame, &model)).unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        assert!(text.contains("Recovered task"));
        assert!(text.contains("recover this exact task"));
    }

    #[test]
    fn short_terminal_modal_does_not_cover_submission_failure_status() {
        use crate::action::Action;
        use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
        for (width, height) in [(32, 8), (48, 9), (80, 24)] {
            let mut model = crate::model::AppModel::new([62; 32]);
            model.editor = Some(Editor {
                kind: EditorKind::ProcessId,
                title: "Attach",
                hint: "Enter a process identity",
                buffer: "a".repeat(32),
                cursor: 32,
                pasted_command: false,
            });
            model.update(Action::TerminalEvent(Event::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            ))));
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| super::super::draw(frame, &model)).unwrap();
            let row = (0..width)
                .map(|column| terminal.backend().buffer()[(column, height - 1)].symbol())
                .collect::<String>();
            assert!(row.starts_with(" Disconnected; draft retained."), "{width}x{height}: {row}");
        }
    }
}
