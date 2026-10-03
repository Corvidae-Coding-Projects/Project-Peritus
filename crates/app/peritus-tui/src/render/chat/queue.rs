//! Paginated input history, with exact row revisions and visibly separate lifecycle labels.

use crate::model::{AppModel, format_id};
use peritus_app_protocol::WorkbenchInputState;
use ratatui::{Frame, layout::Rect};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    super::inspector::draw(
        frame,
        area,
        model,
        content(model),
        " Queue · durable inputs ",
        "Esc back · ↑↓/PgUp/PgDn scroll · Home/End · r refresh",
    );
}

pub(super) fn content(model: &AppModel) -> Vec<String> {
    let panel = &model.chat.workbench;
    let mut lines = vec![panel.message.clone()];
    if let Some(page) = &panel.queue {
        lines.push(format!(
            "{} · revision {} · rows {}–{} of {}",
            if page.query().history() { "Input history" } else { "Pending inputs" },
            page.query().revision(),
            if page.total() == 0 { 0 } else { page.query().offset() + 1 },
            page.query().offset() as usize + page.rows().len(),
            page.total()
        ));
        for (index, row) in page.rows().iter().enumerate() {
            if panel.queue_detail.is_some_and(|selected| selected != row.selected()) {
                continue;
            }
            let state = match row.state() {
                WorkbenchInputState::Queued => "queued (dependencies may delay)",
                WorkbenchInputState::Held => "held",
                WorkbenchInputState::Incorporated => "incorporated into request",
                WorkbenchInputState::Superseded => "superseded · historical text",
                WorkbenchInputState::Withdrawn => "withdrawn · not eligible",
            };
            lines.push(format!(
                "{}. {state} · content rev {}",
                page.query().offset() as usize + index + 1,
                row.selected().revision()
            ));
            lines.push(format!("ID {}", format_id(row.selected().id().as_bytes())));
            if panel.queue_detail.is_some() {
                lines.extend(row.text().as_str().lines().map(str::to_owned));
            } else {
                append_preview(&mut lines, row.text().as_str());
            }
            if !row.dependencies().ids().is_empty() {
                lines.push(format!("Prerequisites: {}", row.dependencies().ids().len()));
            }
        }
    } else {
        lines.push(String::from("No queue page loaded. Inspect with /queue."));
    }
    lines.push(String::from("Esc, then /queue add <text> | edit <row> <text> | hold <row> | release <row> | withdraw <row>"));
    lines.push(String::from("/queue show <row> for exact full text | history | pending | next | previous | correct <row> <text> | order <all pending IDs>"));
    lines
}

fn append_preview(lines: &mut Vec<String>, text: &str) {
    let mut truncated = false;
    for (index, line) in text.lines().enumerate() {
        if index == 3 {
            truncated = true;
            break;
        }
        let preview: String = line.chars().take(160).collect();
        truncated |= preview.len() != line.len();
        lines.push(preview);
    }
    if truncated {
        lines.push(String::from("[Preview only · /queue show <row> reads exact full text]"));
    }
}
