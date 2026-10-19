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
    let mut status = model.chat.snapshot.as_ref().map_or_else(
        || "Ready · Enter sends · Shift-Enter newline · Ctrl-C exits".to_owned(),
        |snapshot| {
            let pending = snapshot.received() > snapshot.incorporated();
            let receipt = format!(
                "Steering: saved r{} · in model request r{}{}",
                snapshot.received(),
                snapshot.incorporated(),
                if pending { " · awaiting next request" } else { "" },
            );
            if model.chat.goal_terminal_status().is_some() {
                format!("{receipt}\nGoal cancelled · /runs details")
            } else if snapshot.snapshot().phase()
                == peritus_app_protocol::ProductRunPhase::RecoveryRequired
            {
                format!("{receipt}\nRecovery required · /retry retries · /runs details")
            } else {
                format!(
                    "{receipt}\n{:?} · Enter {} · Ctrl-C {}",
                    snapshot.snapshot().phase(),
                    if model.chat.active() { "steers" } else { "sends" },
                    if model.chat.active() { "stops" } else { "exits" }
                )
            }
        },
    );
    if model.workbench_chat_pending() {
        status.insert_str(0, "Sending input · draft kept · ");
    }
    status
}

fn render_status(frame: &mut Frame<'_>, area: Rect, model: &AppModel, status: &str) {
    // A transient notice expires after six seconds. Keep the actual connection failure
    // visible for as long as the client is offline, including unattended long runs.
    let history_hint = model.chat.history.hint().unwrap_or_default();
    let notice = model.notice.as_ref().map_or_else(
        || match &model.connection {
            ConnectionStatus::Disconnected(error) => error.as_str(),
            ConnectionStatus::Connecting => "",
            ConnectionStatus::Online { .. } if model.chat.history.is_loading() => {
                "Loading saved messages · draft kept"
            }
            ConnectionStatus::Online { .. } => &history_hint,
        },
        |notice| notice.text.as_str(),
    );
    let [receipt, notice_area] = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Min(1),
        ratatui::layout::Constraint::Length(1),
    ])
    .areas(area);
    frame.render_widget(
        Paragraph::new(crate::sanitize::sanitize_display_text(status))
            .style(Style::default().fg(MUTED))
            .wrap(ratatui::widgets::Wrap { trim: false }),
        receipt,
    );
    frame.render_widget(
        Paragraph::new(Line::styled(
            crate::sanitize::sanitize_display_text(notice),
            Style::default().fg(WARN),
        )),
        notice_area,
    );
}
