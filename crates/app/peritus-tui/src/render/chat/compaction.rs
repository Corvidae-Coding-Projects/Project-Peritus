//! Content-free local compaction preview; exact reply text remains in immutable archives.

use crate::model::{AppModel, format_id};
use ratatui::{Frame, layout::Rect};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    super::inspector::draw(
        frame,
        area,
        model,
        content(model),
        " Compact · local preview ",
        "Esc back · ↑↓/PgUp/PgDn scroll · Home/End · c confirm · r refresh",
    );
}

pub(super) fn content(model: &AppModel) -> Vec<String> {
    let panel = &model.chat.workbench;
    let mut lines = vec![panel.message.clone()];
    if let Some(preview) = &panel.compaction_preview {
        lines.push(format!(
            "Prompt view generation {} · revision {}",
            preview.generation(),
            preview.request().revision()
        ));
        lines.push(String::from("Deterministic local structural reduction only; entries are source handles, not semantic summaries."));
        lines.push(format!(
            "Replace {} older replies: {} → {} bytes",
            preview.entries().len(),
            preview.source_bytes(),
            preview.replacement_bytes()
        ));
        lines.push(format!(
            "Preserved verbatim: {} recent · {} pinned · {} unresolved/unproven · {} without byte saving",
            preview.recent_preserved(),
            preview.pinned_preserved(),
            preview.unresolved_preserved(),
            preview.unsavable_preserved()
        ));
        if let Some(focus) = preview.request().focus() {
            lines.push(format!("User focus preference: {}", focus.as_str()));
        }
        for (index, entry) in preview.entries().iter().enumerate() {
            lines.push(format!(
                "{}. Reply after {} · {} → {} bytes",
                index + 1,
                format_id(entry.invocation().as_bytes()),
                entry.source_bytes(),
                entry.replacement_bytes()
            ));
            lines.push(format!(
                "Source SHA256 {} · handle SHA256 {}",
                hex(entry.source_digest().as_bytes()),
                hex(entry.replacement_digest().as_bytes())
            ));
        }
        lines.push(String::from(if preview.applicable() {
            "Press c to atomically apply this exact preview. Esc cancels; history is unchanged."
        } else {
            "No safe byte-saving reduction is available. Nothing can be confirmed."
        }));
    } else {
        lines.push(String::from("No compaction preview loaded."));
    }
    lines
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}
