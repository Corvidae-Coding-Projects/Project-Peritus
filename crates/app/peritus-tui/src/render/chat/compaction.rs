//! Content-free local compaction preview; exact reply text remains in immutable archives.

use crate::model::{AppModel, format_id};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let panel = &model.chat.workbench;
    let areas = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let mut lines = vec![Line::from(panel.message.clone())];
    if let Some(preview) = &panel.compaction_preview {
        lines.push(Line::from(format!(
            "Prompt view generation {} · revision {}",
            preview.generation(),
            preview.request().revision()
        )));
        lines.push(Line::from("Deterministic local structural reduction only; entries are source handles, not semantic summaries."));
        lines.push(Line::from(format!(
            "Replace {} older replies: {} → {} bytes",
            preview.entries().len(),
            preview.source_bytes(),
            preview.replacement_bytes()
        )));
        lines.push(Line::from(format!(
            "Preserved verbatim: {} recent · {} pinned · {} unresolved/unproven · {} without byte saving",
            preview.recent_preserved(),
            preview.pinned_preserved(),
            preview.unresolved_preserved(),
            preview.unsavable_preserved()
        )));
        if let Some(focus) = preview.request().focus() {
            lines.push(Line::from(format!("User focus preference: {}", focus.as_str())));
        }
        for (index, entry) in preview.entries().iter().enumerate() {
            lines.push(Line::from(format!(
                "{}. Reply after {} · {} → {} bytes",
                index + 1,
                format_id(entry.invocation().as_bytes()),
                entry.source_bytes(),
                entry.replacement_bytes()
            )));
            lines.push(Line::from(format!(
                "Source SHA256 {} · handle SHA256 {}",
                hex(entry.source_digest().as_bytes()),
                hex(entry.replacement_digest().as_bytes())
            )));
        }
        lines.push(Line::from(if preview.applicable() {
            "Press c to atomically apply this exact preview. Esc cancels; history is unchanged."
        } else {
            "No safe byte-saving reduction is available. Nothing can be confirmed."
        }));
    } else {
        lines.push(Line::from("No compaction preview loaded."));
    }
    let content = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(" Compact · local preview "))
        .scroll((u16::try_from(panel.scroll).unwrap_or(u16::MAX), 0));
    frame.render_widget(content, areas[0]);
    frame.render_widget(Paragraph::new("Esc back · ↑↓ scroll · c confirm · r refresh"), areas[1]);
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}
