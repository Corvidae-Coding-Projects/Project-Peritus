//! Paginated input history, with exact row revisions and visibly separate lifecycle labels.

use crate::model::{AppModel, format_id};
use peritus_app_protocol::WorkbenchInputState;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let panel = &model.chat.workbench;
    let sections = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let mut lines = vec![Line::from(panel.message.clone())];
    if let Some(page) = &panel.queue {
        lines.push(Line::from(format!(
            "{} · revision {} · rows {}–{} of {}",
            if page.query().history() { "Input history" } else { "Pending inputs" },
            page.query().revision(),
            if page.total() == 0 { 0 } else { page.query().offset() + 1 },
            page.query().offset() as usize + page.rows().len(),
            page.total()
        )));
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
            lines.push(Line::from(format!(
                "{}. {state} · content rev {}",
                page.query().offset() as usize + index + 1,
                row.selected().revision()
            )));
            lines.push(Line::from(format!("ID {}", format_id(row.selected().id().as_bytes()))));
            if panel.queue_detail.is_some() {
                lines.extend(row.text().as_str().lines().map(|line| Line::from(line.to_owned())));
            } else {
                append_preview(&mut lines, row.text().as_str());
            }
            if !row.dependencies().ids().is_empty() {
                lines
                    .push(Line::from(format!("Prerequisites: {}", row.dependencies().ids().len())));
            }
        }
    } else {
        lines.push(Line::from("No queue page loaded. Inspect with /queue."));
    }
    lines.push(Line::from("Esc, then /queue add <text> | edit <row> <text> | hold <row> | release <row> | withdraw <row>"));
    lines.push(Line::from("/queue show <row> for exact full text | history | pending | next | previous | correct <row> <text> | order <all pending IDs>"));
    let paragraph = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(" Queue · durable inputs "))
        .scroll((u16::try_from(panel.scroll).unwrap_or(u16::MAX), 0));
    frame.render_widget(paragraph, sections[0]);
    frame.render_widget(Paragraph::new("Esc back · ↑↓ scroll · r refresh"), sections[1]);
}

fn append_preview(lines: &mut Vec<Line<'static>>, text: &str) {
    let mut truncated = false;
    for (index, line) in text.lines().enumerate() {
        if index == 3 {
            truncated = true;
            break;
        }
        let preview: String = line.chars().take(160).collect();
        truncated |= preview.len() != line.len();
        lines.push(Line::from(preview));
    }
    if truncated {
        lines.push(Line::from("[Preview only · /queue show <row> reads exact full text]"));
    }
}
