//! Connection-aware composer status stays visible even when provider labels fill the title.

use super::{AppModel, ConnectionStatus, MUTED, WARN};
use ratatui::{Frame, layout::Rect, style::Style, text::Line, widgets::Paragraph};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let status = match &model.connection {
        ConnectionStatus::Disconnected(_) => "Offline · Ctrl-R reconnects · draft kept".to_owned(),
        ConnectionStatus::Connecting => "Connecting · draft kept · Ctrl-Q exits".to_owned(),
        ConnectionStatus::Online { .. } => online_status(model),
    };
    render_status(frame, area, model, &status);
}

fn online_status(model: &AppModel) -> String {
    if !model.chat.expanded && model.chat.working.elapsed_seconds().is_some() {
        "Ctrl-C stops".to_owned()
    } else {
        model.chat.snapshot.as_ref().map_or_else(
            || "Ready · Enter sends · Shift-Enter newline · Ctrl-C exits".to_owned(),
            |snapshot| {
                format!(
                    "{} · input received {} / incorporated {} · Ctrl-C stops / exits",
                    snapshot.snapshot().status(),
                    snapshot.received(),
                    snapshot.incorporated()
                )
            },
        )
    }
}

fn render_status(frame: &mut Frame<'_>, area: Rect, model: &AppModel, status: &str) {
    let notice = model.notice.as_ref().map_or("", |notice| notice.text.as_str());
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                crate::sanitize::sanitize_display_text(status),
                Style::default().fg(MUTED),
            ),
            Line::styled(crate::sanitize::sanitize_display_text(notice), Style::default().fg(WARN)),
        ]),
        area,
    );
}
