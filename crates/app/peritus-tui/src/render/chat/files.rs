//! Exact file/range consent and current selection, with no implied provider delivery.
use crate::model::{AppModel, format_digest, format_id};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let panel = &model.chat.workbench;
    let file = &panel.files;
    let sections = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    if let Some(field) = file.editing {
        let (label, text) = match field {
            0 => ("Workspace-relative path", file.path.as_str()),
            1 => ("Range: all | lines:1:20 | bytes:0:1024", file.range.as_str()),
            _ => ("Caption / instruction", file.caption.as_str()),
        };
        super::images::draw_editor(frame, sections[0], label, text, file.cursor);
        frame.render_widget(
            Paragraph::new("Enter/Esc ends editing · p previews · c confirms"),
            sections[1],
        );
        return;
    }
    let lines = content_lines(model);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(" Files · explicit references "))
            .scroll((u16::try_from(panel.scroll).unwrap_or(u16::MAX), 0)),
        sections[0],
    );
    frame.render_widget(
        Paragraph::new(if file.list {
            "i add · ←→ row · Space select · n/b page · ↑↓ scroll · Esc"
        } else {
            "i path · g range · t caption · m mode · p preview · c confirm · l list"
        }),
        sections[1],
    );
}

fn content_lines(model: &AppModel) -> Vec<Line<'static>> {
    let panel = &model.chat.workbench;
    let file = &panel.files;
    let mut lines = vec![Line::from(panel.message.clone())];
    if file.list {
        if let Some(page) = &file.page {
            lines.push(Line::from(format!(
                "{} retained files · revision {} · page offset {}",
                page.total(),
                page.query().revision(),
                page.query().offset()
            )));
            for (index, row) in page.rows().iter().enumerate() {
                lines.push(Line::from(format!(
                    "{} {} · {:?} · selected={} eligible={}",
                    if index == file.selected { ">" } else { " " },
                    row.label(),
                    row.mode(),
                    row.selected(),
                    row.eligible()
                )));
                lines.push(Line::from(format!(
                    "Reference {} · version {}",
                    format_id(row.attachment().as_bytes()),
                    format_id(row.version().as_bytes())
                )));
                metadata(&mut lines, row.file());
            }
            lines.push(Line::from(
                "Eligible means selected for a future request, not confirmed provider delivery.",
            ));
        } else {
            lines.push(Line::from("Loading retained file references…"));
        }
    } else {
        lines.extend([
            Line::from(format!("Path: {}", file.path)),
            Line::from(format!(
                "Range: {}",
                if file.range.is_empty() { "all" } else { &file.range }
            )),
            Line::from(format!(
                "Mode: {}",
                if file.refresh { "refresh on each request" } else { "immutable snapshot" }
            )),
            Line::from(format!("Caption: {}", file.caption)),
        ]);
        if let Some(preview) = &file.preview {
            metadata(&mut lines, preview.file());
            lines.push(Line::from(format!(
                "Folder identity {}",
                format_digest(preview.folder().as_bytes())
            )));
            lines.push(Line::from(format!(
                "Provider {} · revision {} · model {}",
                format_id(preview.request().provider().as_bytes()),
                preview.provider_revision(),
                preview.resolved_model()
            )));
            lines.push(Line::from("c confirms these exact bytes. If the source changes before confirmation, preview again."));
        } else if let Some(preview) = &file.import_preview {
            metadata(&mut lines, preview.request().file());
            lines.push(Line::from(format!(
                "External immutable snapshot · label {} · upload {}",
                preview.request().selection().path(),
                format_id(preview.request().artifact().as_bytes())
            )));
            lines.push(Line::from(format!(
                "Provider {} · revision {} · model {}",
                format_id(preview.request().selection().provider().as_bytes()),
                preview.provider_revision(),
                preview.resolved_model()
            )));
            lines.push(Line::from(
                "c confirms these uploaded bytes. The original external path will never be reopened.",
            ));
        } else {
            lines.push(Line::from(
                "No preview. Set path/range/caption, then press p. Nothing is included yet.",
            ));
        }
        lines.push(Line::from("UTF-8 text · source ≤64 MiB · selected ≤256 KiB · absolute paths become immutable imports · no inference on confirmation"));
    }
    lines
}
fn metadata(lines: &mut Vec<Line<'static>>, file: peritus_app_protocol::WorkbenchFileMetadata) {
    lines.push(Line::from(format!(
        "{} selected bytes · range {}..{} · source {} bytes",
        file.bytes(),
        file.range().0,
        file.range().1,
        file.source_bytes()
    )));
    lines.push(Line::from(format!("Selected SHA-256 {}", format_digest(file.digest().as_bytes()))));
    lines.push(Line::from(format!(
        "Source SHA-256 {}",
        format_digest(file.source_digest().as_bytes())
    )));
}
