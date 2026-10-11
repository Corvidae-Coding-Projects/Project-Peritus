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
