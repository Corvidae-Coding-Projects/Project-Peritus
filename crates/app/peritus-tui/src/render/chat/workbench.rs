//! Bounded metadata inspector independent of composer and transcript scroll.

use crate::model::{AppModel, format_id};
use ratatui::{Frame, layout::Rect};

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
    if model.chat.workbench.library_open() {
        return super::library::draw(frame, area, model);
    }
    super::inspector::draw(
        frame,
        area,
        model,
        content(model),
        " Sessions · metadata ",
        "Esc back · ↑↓/PgUp/PgDn scroll · Home/End · r refresh",
    );
}

pub(super) fn content(model: &AppModel) -> Vec<String> {
    let panel = &model.chat.workbench;
    if panel.library_open() {
        return super::library::content(model);
    }
    let mut lines = vec![panel.message.clone()];
    if let Some(query) = panel.selected {
        lines.push(format!("ID {}", format_id(query.conversation().as_bytes())));
    }
    if let Some(snapshot) = &panel.snapshot {
        lines.extend([
            format!("Title: {}", snapshot.title().as_str()),
            format!(
                "Revision {} · pinned {} · archived {}",
                snapshot.revision(),
                snapshot.pinned(),
                snapshot.archived()
            ),
        ]);
    } else {
        lines.push(String::from("No metadata snapshot selected."));
    }
    lines.extend([
        String::from("Metadata controls do not start or resume a run."),
        String::from("Esc, then /sessions new <title> or open <id>"),
        String::from("/sessions rename <title> | pin | unpin | archive | unarchive"),
    ]);
    lines
}
