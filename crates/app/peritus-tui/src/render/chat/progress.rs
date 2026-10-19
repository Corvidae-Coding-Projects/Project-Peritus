//! Pinned, observed activity: current operation, file changes, and run accounting.

use super::ACCENT;
use crate::model::AppModel;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::Line,
    widgets::Paragraph,
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel, seconds: u64) {
    let phase = model
        .chat
        .snapshot
        .as_ref()
        .map_or_else(String::new, |snapshot| format!("{:?} · ", snapshot.snapshot().phase()));
    let mut lines = vec![Line::styled(
        format!("Working · {phase}elapsed {seconds}s"),
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )];
    for line in &mut lines {
        for span in &mut line.spans {
            span.content = crate::sanitize::sanitize_display_text(&span.content).into();
        }
    }
    frame.render_widget(Paragraph::new(lines), area);
}
