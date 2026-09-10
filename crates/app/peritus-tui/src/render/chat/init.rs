//! Exact initialization source, command, and file-diff review panel.

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
    if let Some(proposal) = &panel.init {
        lines.push(Line::from(format!(
            "Conversation revision {} · {} observed source(s)",
            proposal.revision(),
            proposal.sources().len()
        )));
        for source in proposal.sources() {
            lines.push(Line::from(format!(
                "source {} · {:?} · {} bytes",
                source.path(),
                source.kind(),
                source.bytes()
            )));
        }
        lines.push(Line::from("Discovered commands · inert and unverified"));
        if proposal.commands().is_empty() {
            lines.push(Line::from("No command candidates discovered."));
        }
        for command in proposal.commands() {
            lines.push(Line::from(format!(
                "{} · {} {} · from {} · {:?}",
                command.kind().as_str(),
                command.executable(),
                command.arguments().join(" "),
                command.source(),
                command.verification()
            )));
        }
        lines.push(Line::from("Exact reviewed instruction-file diff"));
        lines.extend(proposal.patch().diff().lines().map(|line| Line::from(line.to_owned())));
    } else {
        lines.push(Line::from("No initialization proposal loaded."));
    }
    lines.extend([
        Line::from(
            "Applying authorizes only this exact file patch; it never executes listed commands.",
        ),
        Line::from("Esc, then /init apply · /init decline leaves the workspace unchanged"),
    ]);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(" Init · reviewed diff "))
            .scroll((u16::try_from(panel.scroll).unwrap_or(u16::MAX), 0)),
        sections[0],
    );
    frame.render_widget(Paragraph::new("Esc back · ↑↓ scroll · r rediscover"), sections[1]);
}
