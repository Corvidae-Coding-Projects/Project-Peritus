//! Status notices remain completely addressable before optional connection metadata.

use super::{BAD, GOOD, MUTED, WARN};
use crate::model::{AppModel, ConnectionStatus, NoticeLevel, View};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

pub(super) fn render_status(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
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
    let (connection, style) = match &model.connection {
        ConnectionStatus::Connecting => ("connecting".to_owned(), Style::default().fg(WARN)),
        ConnectionStatus::Online { server, downgraded } => {
            let downgrade = if *downgraded { " · negotiated downgrade" } else { "" };
            (
                format!(
                    "online #{} · server {}{downgrade}",
                    model.connection_generation(),
                    crate::sanitize::sanitize_display_text(server)
                ),
                Style::default().fg(GOOD),
            )
        }
        ConnectionStatus::Disconnected(error) => {
            let recovery = if model.product.is_some() {
                "offline · R restart/reconnect"
            } else {
                "offline · R reconnect"
            };
            (
                format!("{recovery} · {}", crate::sanitize::sanitize_display_text(error)),
                Style::default().fg(BAD),
            )
        }
    };
    let readiness = model.daemon_status.as_ref().map_or_else(
        || "readiness unknown".to_owned(),
        |status| format!("{:?}", status.readiness()),
    );
    let mut lines = Vec::new();
    if let Some(notice) = &model.notice {
        let color = match notice.level {
            NoticeLevel::Info => GOOD,
            NoticeLevel::Warning => WARN,
            NoticeLevel::Error => BAD,
        };
        lines.push(Line::styled(
            crate::sanitize::sanitize_display_text(&notice.text),
            Style::default().fg(color),
        ));
    }
    let mut metadata = vec![
        Span::styled(format!(" {connection} "), style.add_modifier(Modifier::BOLD)),
        Span::styled(format!(" {readiness} "), Style::default().fg(MUTED)),
        Span::styled(format!(" cursor {} ", model.last_cursor().get()), Style::default().fg(MUTED)),
        Span::styled(
            format!(" session {} ", model.session_label()),
            Style::default().fg(MUTED),
        ),
    ];
    let help = if model.view == View::Conversation {
        " /help · Ctrl-Q quit "
    } else {
        " ? help · Ctrl-Q quit "
    };
    metadata.push(Span::styled(help, Style::default().fg(MUTED)));
    lines.push(Line::from(metadata));
    super::chat::wrapped_lines(lines, usize::from(width))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn a_long_connection_error_cannot_hide_input_validation_on_a_narrow_screen() {
        let mut model = AppModel::new([1; 32]);
        let _ = model.update(Action::Disconnected("long endpoint failure ".repeat(30)));
        model.notice.as_mut().unwrap().text =
            "ProcessId must contain 32 hexadecimal digits".to_owned();
        let mut terminal = Terminal::new(TestBackend::new(48, 3)).unwrap();
        terminal.draw(|frame| render_status(frame, frame.area(), &model)).unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        assert!(text.contains("ProcessId must contain 32 hexadecimal digits"));
    }
}
