//! Readable exact-byte consent preview; pixels are not required for a terminal confirmation.

use crate::model::{AppModel, format_digest, format_id};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

mod page;

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let panel = &model.chat.workbench;
    let image = &panel.images;
    if image.list {
        page::draw(frame, area, model);
        return;
    }
    let sections = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    if let Some((label, text, cursor)) = image.editor() {
        draw_editor(frame, sections[0], label, text, cursor);
        frame.render_widget(
            Paragraph::new("Enter/Esc ends editing · separate p/c actions"),
            sections[1],
        );
        return;
    }
    let mut lines = vec![Line::from("Explicit local import · no filesystem authority granted")];
    if let Some(preview) = &image.preview {
        let metadata = preview.image();
        let provider = model.product.as_ref().and_then(|product| {
            product
                .launch
                .providers()
                .iter()
                .find(|option| option.profile_id() == preview.request().provider())
        });
        lines.extend([
            Line::from(format!("Image: {}", preview.request().label().as_str())),
            Line::from(format!(
                "{} · {} bytes · {}×{} · {} frame(s)",
                metadata.format().media_type(),
                metadata.bytes(),
                metadata.dimensions().0,
                metadata.dimensions().1,
                metadata.frames()
            )),
            Line::from(format!("SHA-256 {}", format_digest(metadata.digest().as_bytes()))),
            Line::from(format!(
                "Provider: {} · revision {}",
                provider.map_or("Selected profile", crate::runtime::ProductProviderOption::label),
                preview.provider_revision()
            )),
            Line::from(format!("Profile {}", format_id(preview.request().provider().as_bytes()))),
            Line::from(format!(
                "Model: {} · {:?}",
                preview.resolved_model(),
                preview.request().model().effort()
            )),
            Line::from(format!("Conversation revision {}", preview.request().revision())),
            Line::from(
                "Confirm adds these immutable bytes to eligible context. No inference starts here.",
            ),
        ]);
    } else {
        lines.push(Line::from("No confirmed preview. Choose a path (i), then read/preview (p)."));
    }
    lines.extend([
        Line::from(format!("Local source: {}", image.path)),
        Line::from(format!(
            "Caption: {}",
            if image.caption.is_empty() { "(required; press t)" } else { &image.caption }
        )),
        Line::from(panel.message.clone()),
        Line::from("l retained images · x discard preview · r refresh · PgUp/PgDn scroll"),
        Line::from("PNG/JPEG/GIF/WebP · at most 4 MiB · no truncation or clipboard scanning"),
        Line::from(
            "Absolute external paths are imported only as this snapshot. Relative/.. paths reject.",
        ),
    ]);
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(" Attach image · preview / consent "))
        .scroll((u16::try_from(panel.scroll).unwrap_or(u16::MAX), 0));
    frame.render_widget(paragraph, sections[0]);
    frame.render_widget(Paragraph::new("Esc back · i file · t text · p view · c OK"), sections[1]);
}

pub(super) fn draw_editor(
    frame: &mut Frame<'_>,
    area: Rect,
    label: &str,
    text: &str,
    cursor: usize,
) {
    let layout =
        super::composer::layout(text, cursor, None, usize::from(area.width.saturating_sub(2)));
    let rows = usize::from(area.height.saturating_sub(2)).max(1);
    let offset = layout.row.saturating_sub(rows - 1);
    frame.render_widget(
        Paragraph::new(layout.lines.into_iter().skip(offset).take(rows).collect::<Vec<_>>())
            .block(Block::default().borders(Borders::ALL).title(label)),
        area,
    );
    if area.height > 2 && area.width > 2 {
        frame.set_cursor_position((
            area.x + 1 + u16::try_from(layout.column).unwrap_or(0).min(area.width - 3),
            area.y + 1 + u16::try_from(layout.row - offset).unwrap_or(0).min(area.height - 3),
        ));
    }
}
