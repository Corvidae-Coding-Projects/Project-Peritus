//! Session identities remain selectable and visible independently of the metadata inspector.

use crate::model::{AppModel, format_id};
use peritus_app_protocol::ConversationLibraryItem;
use ratatui::{Frame, layout::Rect};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    super::inspector::draw(
        frame,
        area,
        model,
        content(model),
        " Sessions ",
        "↑↓ select · Enter open · n/p pages · r refresh · Esc back",
    );
}

fn header(model: &AppModel) -> Vec<String> {
    let panel = &model.chat.workbench;
    let mut lines = vec![panel.message.clone()];
    if let Some(page) = &panel.library {
        let start =
            if page.items().is_empty() { 0 } else { page.query().offset().saturating_add(1) };
        lines.push(format!(
            "Showing {start}–{} of {} · PgUp/PgDn scroll details",
            page.query()
                .offset()
                .saturating_add(u32::try_from(page.items().len()).unwrap_or(u32::MAX)),
            page.total()
        ));
        if let Some(literal) = page.query().literal() {
            lines.push(format!("Search: {}", literal.as_str()));
        }
        if page.items().is_empty() {
            lines.push(
                "No matching conversations. Esc returns to the composer to change your search."
                    .into(),
            );
        }
    }
    lines
}

pub(super) fn content(model: &AppModel) -> Vec<String> {
    let mut lines = header(model);
    if let Some(page) = &model.chat.workbench.library {
        for (index, item) in page.items().iter().enumerate() {
            lines.extend(item_lines(item, index == model.chat.workbench.library_selected));
        }
    }
    lines
}

fn item_lines(item: &ConversationLibraryItem, selected: bool) -> Vec<String> {
    let mut lines = vec![
        format!(
            "{} {}{}{}{}",
            if selected { "▶" } else { " " },
            item.title().as_str(),
            if item.pinned() { " · pinned" } else { "" },
            if item.archived() { " · archived" } else { "" },
            if item.goal_draft() { " · goal draft" } else { "" }
        ),
        format!("  ID {}", format_id(item.query().conversation().as_bytes())),
    ];
    if let Some(snippet) = item.snippet() {
        lines.push(format!("  {}", snippet.text()));
    }
    if !item.handoff().is_empty() {
        lines.push(format!("  {}", item.handoff()));
    }
    if let Some(branch) = item.branch() {
        lines.push(format!(
            "  fork of {} at checkpoint {}",
            format_id(branch.parent().conversation().as_bytes()),
            format_id(branch.checkpoint().as_bytes())
        ));
    }
    lines
}

pub fn selection_scroll(model: &AppModel) -> usize {
    if model.chat.workbench.library_selected == 0 {
        return 0;
    }
    let mut prefix = header(model);
    if let Some(page) = &model.chat.workbench.library {
        for item in page.items().iter().take(model.chat.workbench.library_selected) {
            prefix.extend(item_lines(item, false));
        }
    }
    let width = model.chat.viewport.map_or(80, |area| area.width);
    super::inspector::wrap(prefix, width.saturating_sub(2))
        .len()
        .min(super::inspector::scroll_limit(model))
}
