//! Exact retained reference details; selection and next-turn eligibility are distinct facts.

use crate::model::{AppModel, format_digest, format_id};
use ratatui::{Frame, layout::Rect};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    super::super::inspector::draw(
        frame,
        area,
        model,
        content(model),
        " Images · retained references ",
        "Esc back · ←→ image · ↑↓/PgUp/PgDn scroll · Home/End · Space",
    );
}

pub(super) fn content(model: &AppModel) -> Vec<String> {
    let panel = &model.chat.workbench;
    let image = &panel.images;
    let mut lines =
        vec![String::from("Retained images · Space changes future selection; no inference starts")];
    if let Some(page) = &image.page {
        lines.push(format!(
            "Revision {} · {} total · page offset {}",
            page.query().revision(),
            page.total(),
            page.query().offset()
        ));
        if let Some(row) = page.rows().get(image.selected) {
            let metadata = row.image();
            lines.extend([
                format!(
                    "Image {} of page {}: {}",
                    image.selected + 1,
                    page.rows().len(),
                    row.label().as_str()
                ),
                format!(
                    "Selected: {} · eligible next turn: {} · caption {:?}",
                    row.selected(),
                    row.eligible(),
                    row.source().state()
                ),
                format!(
                    "{} · {} bytes · {}×{} · {} frame(s)",
                    metadata.format().media_type(),
                    metadata.bytes(),
                    metadata.dimensions().0,
                    metadata.dimensions().1,
                    metadata.frames()
                ),
                format!("SHA-256 {}", format_digest(metadata.digest().as_bytes())),
                format!("Validation: {}", metadata.validation().description()),
                format!("Import {}", format_id(row.operation().as_bytes())),
                format!("Artifact {}", format_id(row.artifact().as_bytes())),
                format!(
                    "Caption source {} · content revision {}",
                    format_id(row.source().selected().id().as_bytes()),
                    row.source().selected().revision()
                ),
                format!("Caption: {}", row.source().text().as_str()),
                String::from("/queue inspects and edits the caption source/history."),
            ]);
        } else {
            lines.push(String::from("No retained images on this page. Press i to import."));
        }
        lines.push(String::from("Eligibility is not proof of provider delivery. /context records exact sealed inclusion. Held/withdrawn captions are excluded even if selected."));
    } else {
        lines.push(String::from("No current image page. Press r to inspect."));
    }
    lines.push(String::from(
        "i import · r refresh · n/b pages · PgUp/PgDn scroll · /queue edits captions",
    ));
    lines.push(panel.message.clone());
    lines
}
