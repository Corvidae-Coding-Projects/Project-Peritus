//! Status notices stay visible before optional connection metadata.

use super::{BAD, GOOD, MUTED, WARN, short_text};
use crate::model::{AppModel, ConnectionStatus, NoticeLevel, View};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

pub(super) fn render_status(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let (connection, style) = match &model.connection {
        ConnectionStatus::Connecting => ("connecting".to_owned(), Style::default().fg(WARN)),
        ConnectionStatus::Online { .. } => {
            (format!("online #{}", model.connection_generation()), Style::default().fg(GOOD))
        }
        ConnectionStatus::Disconnected(error) => {
            let recovery = if model.product.is_some() {
                "offline · R restart/reconnect"
            } else {
                "offline · R reconnect"
            };
            (format!("{recovery} · {error}"), Style::default().fg(BAD))
        }
    };
    let readiness = model.daemon_status.as_ref().map_or_else(
        || "readiness unknown".to_owned(),
        |status| format!("{:?}", status.readiness()),
    );
    let mut spans = vec![
        Span::styled(format!(" {connection} "), style.add_modifier(Modifier::BOLD)),
        Span::styled(format!(" {readiness} "), Style::default().fg(MUTED)),
        Span::styled(format!(" cursor {} ", model.last_cursor().get()), Style::default().fg(MUTED)),
        Span::styled(
            format!(" session {} ", short_text(&model.session_label(), 12)),
            Style::default().fg(MUTED),
        ),
    ];
    if let Some(notice) = &model.notice {
        let color = match notice.level {
            NoticeLevel::Info => GOOD,
            NoticeLevel::Warning => WARN,
            NoticeLevel::Error => BAD,
        };
        spans.insert(0, Span::styled(format!(" {} · ", notice.text), Style::default().fg(color)));
    } else {
        let help = if model.view == View::Conversation {
            " /help · Ctrl-Q quit "
        } else {
            " ? help · Ctrl-Q quit "
        };
        spans.push(Span::styled(help, Style::default().fg(MUTED)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
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
        let mut terminal = Terminal::new(TestBackend::new(48, 1)).unwrap();
        terminal.draw(|frame| render_status(frame, frame.area(), &model)).unwrap();
        let row = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect::<String>();
        assert!(row.starts_with(" ProcessId must contain 32 hexadecimal digits"));
    }
}
