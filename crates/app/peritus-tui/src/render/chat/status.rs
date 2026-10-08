//! Connection-aware composer status remains completely addressable below the draft.

use super::{AppModel, ConnectionStatus, MUTED, WARN};
use peritus_app_protocol::ProductRunControlAction;
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let lines = status_lines(model, area.width);
    let (maximum, page) = scroll_metrics_for(lines.len(), area.height);
    let offset = model.status_scroll.min(maximum);
    let end = offset.saturating_add(page).min(lines.len());
    let title = format!(
        " Status rows {}–{} / {} · Alt+PgUp/PgDn/Home/End ",
        offset.saturating_add(1).min(lines.len()),
        end,
        lines.len()
    );
    frame.render_widget(
        Paragraph::new(lines.into_iter().skip(offset).take(page).collect::<Vec<_>>())
            .block(Block::default().borders(Borders::TOP).title(title)),
        area,
    );
}

pub(super) fn scroll_metrics(model: &AppModel, area: Rect) -> (usize, usize) {
    scroll_metrics_for(status_lines(model, area.width).len(), area.height)
}

fn scroll_metrics_for(lines: usize, height: u16) -> (usize, usize) {
    let page = usize::from(height.saturating_sub(1)).max(1);
    (lines.saturating_sub(page), page)
}

fn status_lines(model: &AppModel, width: u16) -> Vec<Line<'static>> {
    let status = match &model.connection {
        ConnectionStatus::Disconnected(_) => "Offline · Ctrl-R reconnects · draft kept".to_owned(),
        ConnectionStatus::Connecting => "Connecting · draft kept · Ctrl-Q exits".to_owned(),
        ConnectionStatus::Online { .. } => online_status(model),
    };
    let mut lines = vec![Line::styled(
        crate::sanitize::sanitize_display_text(&status),
        Style::default().fg(MUTED),
    )];
    if let Some(notice) = &model.notice {
        lines.push(Line::styled(
            crate::sanitize::sanitize_display_text(&notice.text),
            Style::default().fg(WARN),
        ));
    }
    super::wrapped_lines(lines, usize::from(width))
}

fn online_status(model: &AppModel) -> String {
    if model.chat.snapshot.as_ref().is_some_and(|snapshot| {
        snapshot.snapshot().phase() == peritus_app_protocol::ProductRunPhase::RecoveryRequired
    }) {
        return if model.chat_control_is_legal(ProductRunControlAction::Retry) {
            "Recovery required · /retry retries this exact run · /runs opens legal controls and details"
                .to_owned()
        } else {
            "Recovery required · exact retry is not currently legal · /runs opens legal controls and details"
                .to_owned()
        };
    }
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
