//! Effective host-intersected workspace permission projection.

use crate::model::AppModel;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let panel = &model.chat.workbench;
    let sections = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let mut lines = vec![Line::from(panel.message.clone())];
    if let Some(permissions) = &panel.permissions {
        lines.push(Line::from(format!(
            "{} · conversation revision {} · policy revision {}",
            permissions.trust().as_str(),
            permissions.conversation_revision(),
            permissions.authority_revision()
        )));
        for entry in permissions.entries() {
            let approval = if entry.explicit_approval_still_required() {
                " · exact operation approval still required"
            } else {
                ""
            };
            lines.push(Line::from(format!(
                "{:<7} effective={} · host={} · {}{}",
                entry.capability().as_str(),
                state(entry.effective_allowed()),
                state(entry.host_allowed()),
                entry.provenance().as_str(),
                approval
            )));
        }
    } else {
        lines.push(Line::from("No permission inspection loaded."));
    }
    lines.extend([
        Line::from("This overlay can narrow host authority, never broaden it."),
        Line::from(
            "Every lower path, lease, sandbox, and signed-operation gate remains mandatory.",
        ),
        Line::from("Esc, then /permissions restrict|grant read|write|process|network"),
    ]);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(" Permissions · enforced "))
            .scroll((u16::try_from(panel.scroll).unwrap_or(u16::MAX), 0)),
        sections[0],
    );
    frame.render_widget(Paragraph::new("Esc back · ↑↓ scroll · r refresh"), sections[1]);
}

const fn state(allowed: bool) -> &'static str {
    if allowed { "allowed" } else { "denied" }
}
