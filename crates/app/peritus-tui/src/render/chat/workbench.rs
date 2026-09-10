//! Bounded metadata inspector independent of composer and transcript scroll.

use crate::model::{AppModel, format_id};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    if model.chat.workbench.files.open {
        return super::files::draw(frame, area, model);
    }
    if model.chat.workbench.images.open {
        return super::images::draw(frame, area, model);
    }
    if model.chat.workbench.goal_mode {
        return super::goal::draw(frame, area, model);
    }
    if model.chat.workbench.brief_open() {
        return super::brief::draw(frame, area, model);
    }
    if model.chat.workbench.permissions_open() {
        return super::permissions::draw(frame, area, model);
    }
    if model.chat.workbench.init_open() {
        return super::init::draw(frame, area, model);
    }
    if model.chat.workbench.memory_open() {
        return super::memory::draw(frame, area, model);
    }
    if model.chat.workbench.compaction_open() {
        return super::compaction::draw(frame, area, model);
    }
    if model.chat.workbench.checkpoints_open() {
        return super::checkpoints::draw(frame, area, model);
    }
    if model.chat.workbench.context_mode.is_some() {
        return super::context::draw(frame, area, model);
    }
    if model.chat.workbench.queue_open() {
        return super::queue::draw(frame, area, model);
    }
    let panel = &model.chat.workbench;
    let sections = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let mut lines = vec![Line::from(panel.message.clone())];
    if let Some(page) = &panel.library {
        for item in page.items() {
            let flags = format!(
                "{}{}{}",
                if item.pinned() { " pinned" } else { "" },
                if item.archived() { " archived" } else { "" },
                if item.goal_draft() { " goal-draft" } else { "" },
            );
            lines.push(Line::from(format!(
                "{}  {}{}",
                format_id(item.query().conversation().as_bytes()),
                item.title().as_str(),
                flags,
            )));
            if let Some(snippet) = item.snippet() {
                lines.push(Line::from(format!("  {}", snippet.text())));
            }
            if let Some(branch) = item.branch() {
                lines.push(Line::from(format!(
                    "  fork of {} at checkpoint {}",
                    format_id(branch.parent().conversation().as_bytes()),
                    format_id(branch.checkpoint().as_bytes()),
                )));
            }
        }
    }
    if let Some(query) = panel.selected {
        lines.push(Line::from(format!("ID {}", format_id(query.conversation().as_bytes()))));
    }
    if let Some(snapshot) = &panel.snapshot {
        lines.extend([
            Line::from(format!("Title: {}", snapshot.title().as_str())),
            Line::from(format!(
                "Revision {} · pinned {} · archived {}",
                snapshot.revision(),
                snapshot.pinned(),
                snapshot.archived()
            )),
        ]);
    } else {
        lines.push(Line::from("No metadata snapshot selected."));
    }
    lines.extend([
        Line::from("Metadata controls do not start or resume a run."),
        Line::from("Esc, then /sessions new <title> or open <id>"),
        Line::from("/sessions rename <title> | pin | unpin | archive | unarchive"),
    ]);
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(" Sessions · metadata "))
        .scroll((u16::try_from(panel.scroll).unwrap_or(u16::MAX), 0));
    frame.render_widget(paragraph, sections[0]);
    frame.render_widget(Paragraph::new("Esc back · ↑↓ scroll · r refresh"), sections[1]);
}
