//! Exact retained reference details; selection and next-turn eligibility are distinct facts.

use crate::model::{AppModel, format_digest, format_id};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let panel = &model.chat.workbench;
    let image = &panel.images;
    let sections = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let mut lines =
        vec![Line::from("Retained images · Space changes future selection; no inference starts")];
    if let Some(page) = &image.page {
        lines.push(Line::from(format!(
            "Revision {} · {} total · page offset {}",
            page.query().revision(),
            page.total(),
            page.query().offset()
        )));
        if let Some(row) = page.rows().get(image.selected) {
            let metadata = row.image();
            lines.extend([
                Line::from(format!(
                    "Image {} of page {}: {}",
                    image.selected + 1,
                    page.rows().len(),
                    row.label().as_str()
                )),
                Line::from(format!(
                    "Selected: {} · eligible next turn: {} · caption {:?}",
                    row.selected(),
                    row.eligible(),
                    row.source().state()
                )),
                Line::from(format!(
                    "{} · {} bytes · {}×{} · {} frame(s)",
                    metadata.format().media_type(),
                    metadata.bytes(),
                    metadata.dimensions().0,
                    metadata.dimensions().1,
                    metadata.frames()
                )),
                Line::from(format!("SHA-256 {}", format_digest(metadata.digest().as_bytes()))),
                Line::from(format!("Import {}", format_id(row.operation().as_bytes()))),
                Line::from(format!("Artifact {}", format_id(row.artifact().as_bytes()))),
                Line::from(format!(
                    "Caption source {} · content revision {}",
                    format_id(row.source().selected().id().as_bytes()),
                    row.source().selected().revision()
                )),
                Line::from(format!("Caption: {}", row.source().text().as_str())),
                Line::from("/queue inspects and edits the caption source/history."),
            ]);
        } else {
            lines.push(Line::from("No retained images on this page. Press i to import."));
        }
        lines.push(Line::from("Eligibility is not proof of provider delivery. /context records exact sealed inclusion. Held/withdrawn captions are excluded even if selected."));
    } else {
        lines.push(Line::from("No current image page. Press r to inspect."));
    }
    lines.push(Line::from(
        "i import · r refresh · n/b pages · PgUp/PgDn scroll · /queue edits captions",
    ));
    lines.push(Line::from(panel.message.clone()));
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(" Images · retained references "))
            .scroll((u16::try_from(panel.scroll).unwrap_or(u16::MAX), 0)),
        sections[0],
    );
    frame.render_widget(Paragraph::new("Esc back · ←→ image · ↑↓ scroll · Space"), sections[1]);
}
