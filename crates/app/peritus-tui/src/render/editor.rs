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
