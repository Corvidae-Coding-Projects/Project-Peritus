//! Paginated input history, with exact row revisions and visibly separate lifecycle labels.

use crate::{
    input::composer,
    model::{AppModel, format_id},
};
use peritus_app_protocol::WorkbenchInputState;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    if model.chat.workbench.queue_editor.is_some() {
        draw_editor(frame, area, model);
        return;
    }
    if model.chat.workbench.queue_detail.is_some() && model.chat.workbench.queue_withdraw.is_none()
    {
        super::inspector::draw(
            frame,
            area,
            model,
            content(model),
            " Queue · full saved input ",
            "Esc list · ↑↓ select input · PgUp/PgDn scroll · e edit · h hold/release · Delete withdraw",
        );
        return;
    }
    let panel = &model.chat.workbench;
    let sections = Layout::vertical([
        Constraint::Length(if panel.queue_withdraw.is_some() { 4 } else { 3 }),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .split(area);
    let mut header = vec![Line::from(panel.message.clone())];
    if let Some(page) = &panel.queue {
        header.push(Line::from(format!(
            "{} · revision {} · page rows {}–{} of {}",
            if page.query().history() { "Input history" } else { "Pending inputs" },
            page.query().revision(),
            if page.total() == 0 { 0 } else { page.query().offset() + 1 },
            page.query().offset() as usize + page.rows().len(),
            page.total(),
        )));
    }
    if let Some(target) = panel.queue_withdraw {
        header.push(Line::styled(
            format!(
                "Withdraw {} revision {}? Enter confirms; Esc cancels.",
                format_id(target.selected.id().as_bytes()),
                target.selected.revision(),
            ),
            Style::default().fg(super::WARN).add_modifier(Modifier::BOLD),
        ));
    }
    frame.render_widget(Paragraph::new(header), sections[0]);
    let items = panel.queue.as_ref().map_or_else(Vec::new, |page| {
        page.rows()
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let mut text = vec![format!(
                    "{}. {} · revision {}",
                    page.query().offset() as usize + index + 1,
                    state_label(row.state()),
                    row.selected().revision(),
                )];
                append_preview(&mut text, row.text().as_str());
                let lines = super::wrapped_lines(
                    text.into_iter().map(Line::from).collect(),
                    usize::from(sections[1].width.saturating_sub(4)),
                );
                ListItem::new(lines)
            })
            .collect::<Vec<_>>()
    });
    let mut selected = ListState::default();
    if let Some(page) = &panel.queue
        && !page.rows().is_empty()
    {
        selected.select(Some(panel.queue_selected.min(page.rows().len() - 1)));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).title(" Queue · durable inputs "))
            .highlight_symbol("▸ ")
            .highlight_style(Style::default().fg(super::ACCENT).add_modifier(Modifier::BOLD)),
        sections[1],
        &mut selected,
    );
    frame.render_widget(Paragraph::new(vec![
        Line::from("↑↓ select · Enter full text · e edit · h hold/release · Delete withdraw · Alt+↑↓ move"),
        Line::from("n/p page · r refresh · Home/End select · Esc back · main message draft is preserved"),
    ]), sections[2]);
}

fn draw_editor(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let panel = &model.chat.workbench;
    let Some(editor) = &panel.queue_editor else { return };
    let sections =
        Layout::vertical([Constraint::Length(3), Constraint::Min(1), Constraint::Length(2)])
            .split(area);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!(
                "Editing saved input {} · content revision {}",
                format_id(editor.target.selected.id().as_bytes()),
                editor.target.selected.revision()
            )),
            Line::from(panel.message.clone()),
            Line::from("This is a separate queue draft. Your main message stays intact."),
        ]),
        sections[0],
    );
    let selection = crate::input::selection::range(editor.anchor, editor.cursor);
    let rows = usize::from(sections[1].height.saturating_sub(2)).max(1);
    let draft = composer::layout_for_rows(
        &editor.text,
        editor.cursor,
        selection.as_ref(),
        usize::from(sections[1].width.saturating_sub(2)),
        rows,
    );
    let offset = draft.offset(sections[1]);
    frame.render_widget(
        Paragraph::new(draft.lines.into_iter().skip(offset).take(rows).collect::<Vec<_>>()).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(super::ACCENT))
                .title(" Edit pending instruction "),
        ),
        sections[1],
    );
    if sections[1].width >= 3 && sections[1].height >= 3 {
        frame.set_cursor_position((
            sections[1].x
                + 1
                + u16::try_from(draft.column)
                    .unwrap_or(u16::MAX)
                    .min(sections[1].width.saturating_sub(3)),
            sections[1].y
                + 1
                + u16::try_from(draft.row.saturating_sub(offset))
                    .unwrap_or(u16::MAX)
                    .min(sections[1].height.saturating_sub(3)),
        ));
    }
    frame.render_widget(Paragraph::new(vec![
        Line::from("Enter saves · Shift+Enter newline · Esc cancels · arrows/Home/End edit · Shift selects"),
        Line::from("Paste inserts only · saving never sends a model request · a durable receipt confirms acceptance"),
    ]), sections[2]);
}

fn state_label(state: WorkbenchInputState) -> &'static str {
    match state {
        WorkbenchInputState::Queued => "queued (dependencies may delay)",
        WorkbenchInputState::Held => "held",
        WorkbenchInputState::Incorporated => "incorporated into request",
        WorkbenchInputState::Superseded => "superseded · historical text",
        WorkbenchInputState::Withdrawn => "withdrawn · not eligible",
    }
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
            let state = state_label(row.state());
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
    lines.push(String::from("Esc, then /queue start runs eligible saved input · /queue add <text> | edit <row> <text> | hold <row> | release <row> | withdraw <row>"));
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
        lines.push(String::from("[Preview only · Enter shows exact full text]"));
    }
}
