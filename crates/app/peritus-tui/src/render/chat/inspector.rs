//! Shared wrapping and bounds for the metadata inspectors; rendering and keys use the same rows.

use crate::{input::composer, model::AppModel};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

pub(super) fn draw(
    frame: &mut Frame<'_>,
    area: Rect,
    model: &AppModel,
    content: Vec<String>,
    title: &'static str,
    footer: &'static str,
) {
    let sections = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let rows = usize::from(sections[0].height.saturating_sub(2));
    let lines = wrap(content, sections[0].width.saturating_sub(2));
    let offset = model.chat.workbench.scroll.min(lines.len().saturating_sub(rows));
    frame.render_widget(
        Paragraph::new(lines.into_iter().skip(offset).take(rows).collect::<Vec<_>>())
            .block(Block::default().borders(Borders::ALL).title(title)),
        sections[0],
    );
    frame.render_widget(Paragraph::new(footer), sections[1]);
}

pub fn scroll_limit(model: &AppModel) -> usize {
    let panel = &model.chat.workbench;
    let content = if panel.files.open {
        super::files::content_lines(model)
    } else if panel.images.open {
        super::images::content(model)
    } else if panel.goal_mode {
        super::goal::content(model)
    } else if panel.brief_open() {
        super::brief::content(model)
    } else if panel.permissions_open() {
        super::permissions::content(model)
    } else if panel.init_open() {
        super::init::content(model)
    } else if panel.memory_open() {
        super::memory::content(model)
    } else if panel.compaction_open() {
        super::compaction::content(model)
    } else if panel.checkpoints_open() {
        super::checkpoints::content(model)
    } else if panel.context_mode.is_some() {
        super::context::content(model)
    } else if panel.queue_open() {
        super::queue::content(model)
    } else {
        super::workbench::content(model)
    };
    let area = panel_area(model);
    let sections = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    wrap(content, sections[0].width.saturating_sub(2))
        .len()
        .saturating_sub(usize::from(sections[0].height.saturating_sub(2)))
}

pub(super) fn wrap(content: Vec<String>, width: u16) -> Vec<Line<'static>> {
    let lines = content
        .into_iter()
        .flat_map(|line| {
            line.split('\n').map(|text| Line::from(text.to_owned())).collect::<Vec<_>>()
        })
        .collect();
    super::wrapped_lines(lines, usize::from(width))
}

pub(super) fn panel_area(model: &AppModel) -> Rect {
    let viewport = model.chat.viewport.unwrap_or(Rect::new(0, 0, 80, 24));
    let draft = composer::layout(
        &model.chat.buffer,
        model.chat.cursor,
        model.chat.selection().as_ref(),
        usize::from(viewport.width.saturating_sub(2)),
    );
    let regions = composer::regions(
        viewport,
        draft.lines.len(),
        model.chat.working.elapsed_seconds().is_some(),
    );
    regions[1]
}
