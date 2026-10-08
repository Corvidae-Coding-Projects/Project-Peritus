//! Exact file/range consent and current selection, with no implied provider delivery.
use crate::model::{AppModel, format_digest, format_id};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    widgets::Paragraph,
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
        if field == 0 {
            // Editing offsets refer to the exact native text, never its display escape.
            let cursor = text[..file.cursor].escape_debug().to_string().len();
            super::images::draw_editor(
                frame,
                sections[0],
                label,
                &text.escape_debug().to_string(),
                cursor,
            );
        } else {
            super::images::draw_editor(frame, sections[0], label, text, file.cursor);
        }
        frame.render_widget(
            Paragraph::new("Enter/Esc ends editing · p previews · c confirms"),
            sections[1],
        );
        return;
    }
    super::inspector::draw(
        frame,
        area,
        model,
        content_lines(model),
        " Files · explicit references ",
        if file.list {
            "i add · ←→ row · Space select · n/b page · ↑↓/PgUp/PgDn · Home/End · Esc"
        } else {
            "i path · g range · t caption · m mode · p preview · c confirm · l list"
        },
    );
}

pub(super) fn content_lines(model: &AppModel) -> Vec<String> {
    let panel = &model.chat.workbench;
    let file = &panel.files;
    let mut lines = vec![panel.message.clone()];
    if file.list {
        if let Some(page) = &file.page {
            lines.push(format!(
                "{} retained files · revision {} · page offset {}",
                page.total(),
                page.query().revision(),
                page.query().offset()
            ));
            for (index, row) in page.rows().iter().enumerate() {
                lines.push(format!(
                    "{} {} · {:?} · selected={} eligible={}",
                    if index == file.selected { ">" } else { " " },
                    row.label().escape_debug(),
                    row.mode(),
                    row.selected(),
                    row.eligible()
                ));
                lines.push(format!(
                    "Reference {} · version {}",
                    format_id(row.attachment().as_bytes()),
                    format_id(row.version().as_bytes())
                ));
                metadata(&mut lines, row.file());
            }
            lines.push(String::from(
                "Eligible means selected for a future request, not confirmed provider delivery.",
            ));
        } else {
            lines.push(String::from("Loading retained file references…"));
        }
    } else {
        lines.extend([
            format!("Path: {}", file.path.escape_debug()),
            format!("Range: {}", if file.range.is_empty() { "all" } else { &file.range }),
            format!(
                "Mode: {}",
                if file.refresh { "refresh on each request" } else { "immutable snapshot" }
            ),
            format!("Caption: {}", file.caption),
        ]);
        if let Some(preview) = &file.preview {
            metadata(&mut lines, preview.file());
            lines.push(format!("Folder identity {}", format_digest(preview.folder().as_bytes())));
            lines.push(format!(
                "Provider {} · revision {} · model {}",
                format_id(preview.request().provider().as_bytes()),
                preview.provider_revision(),
                preview.resolved_model()
            ));
            lines.push(String::from("c confirms these exact bytes. If the source changes before confirmation, preview again."));
        } else if let Some(preview) = &file.import_preview {
            metadata(&mut lines, preview.request().file());
            lines.push(format!(
                "External immutable snapshot · label {} · upload {}",
                preview.request().selection().path().escape_debug(),
                format_id(preview.request().artifact().as_bytes())
            ));
            lines.push(format!(
                "Provider {} · revision {} · model {}",
                format_id(preview.request().selection().provider().as_bytes()),
                preview.provider_revision(),
                preview.resolved_model()
            ));
            lines.push(String::from(
                "c confirms these uploaded bytes. The original external path will never be reopened.",
            ));
        } else {
            lines.push(String::from(
                "No preview. Set path/range/caption, then press p. Nothing is included yet.",
            ));
        }
        lines.push(String::from("UTF-8 text · external source inspection ≤64 MiB · absolute paths become immutable imports · no inference on confirmation"));
    }
    lines
}
fn metadata(lines: &mut Vec<String>, file: peritus_app_protocol::WorkbenchFileMetadata) {
    lines.push(format!(
        "{} selected bytes · range {}..{} · source {} bytes",
        file.bytes(),
        file.range().0,
        file.range().1,
        file.source_bytes()
    ));
    lines.push(format!("Selected SHA-256 {}", format_digest(file.digest().as_bytes())));
    lines.push(format!("Source SHA-256 {}", format_digest(file.source_digest().as_bytes())));
}
