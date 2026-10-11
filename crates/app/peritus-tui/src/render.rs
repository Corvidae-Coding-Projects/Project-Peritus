//! Ratatui rendering for every G2 interaction view.

mod chat;
pub use chat::{transcript_area, transcript_rows};
mod editor;
mod product;
mod prompts;
mod status;
mod tabs;
pub use chat::doctor::scroll_limit as doctor_scroll_limit;
pub use chat::inspector::scroll_limit as workbench_scroll_limit;
pub use chat::library::selection_scroll as library_selection_scroll;
use editor::render_editor;
use status::render_status;
use tabs::render_tabs;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::model::{AppModel, View, format_digest, format_id};

const ACCENT: Color = Color::Rgb(196, 128, 255);
const MUTED: Color = Color::Rgb(135, 145, 160);
const GOOD: Color = Color::Rgb(92, 200, 142);
const WARN: Color = Color::Rgb(238, 190, 94);
const BAD: Color = Color::Rgb(244, 105, 125);

pub fn draw(frame: &mut Frame<'_>, model: &AppModel) {
    if model.view == View::Conversation {
        chat::draw(frame, model);
        if let Some(editor) = &model.editor {
            render_editor(frame, editor);
        }
        return;
    }
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(4), Constraint::Length(1)])
        .split(frame.area());
    render_tabs(frame, regions[0], model);
    match model.view {
        View::Conversation => {}
        View::Runs if model.product.is_some() => product::dashboard(frame, regions[1], model),
        View::Diff if model.product.is_some() => product::diff(frame, regions[1], model),
        View::Review if model.product.is_some() => product::review(frame, regions[1], model),
        View::Preview => product::preview(frame, regions[1], model),
        View::Runs | View::Diff | View::Review | View::Trace | View::Evolution => {
            render_event_view(frame, regions[1], model);
        }
        View::Terminal => render_terminal(frame, regions[1], model),
        View::Approvals => prompts::render(frame, regions[1], model),
        View::Help => render_help(frame, regions[1], model),
    }
    render_status(frame, regions[2], model);
    if let Some(editor) = &model.editor {
        render_editor(frame, editor);
    }
}

pub fn inspection_scroll_limit(model: &AppModel) -> usize {
    let area = model.chat.viewport.unwrap_or(Rect::new(0, 0, 80, 24));
    let regions =
        Layout::vertical([Constraint::Length(3), Constraint::Min(4), Constraint::Length(1)])
            .split(area);
    if model.view == View::Approvals {
        prompts::scroll_limit(model, regions[1])
    } else {
        product::scroll_limit(model, regions[1])
    }
}

pub fn inspection_scroll_page(model: &AppModel) -> usize {
    if model.view != View::Runs {
        return 12;
    }
    let area = model.chat.viewport.unwrap_or(Rect::new(0, 0, 80, 24));
    let regions =
        Layout::vertical([Constraint::Length(3), Constraint::Min(4), Constraint::Length(1)])
            .split(area);
    product::scroll_page(regions[1])
}

fn render_event_view(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
        .split(area);
    let visible = model.visible_event_indices();
    let items = if visible.is_empty() {
        vec![ListItem::new(Line::styled(
            "No matching live events received",
            Style::default().fg(MUTED),
        ))]
    } else {
        visible
            .iter()
            .filter_map(|index| model.events.get(*index))
            .map(|record| ListItem::new(record.summary()))
            .collect()
    };
    let mut state = ListState::default();
    if !visible.is_empty() {
        let selected = model
            .selected_event
            .and_then(|selected| visible.iter().position(|index| *index == selected))
            .unwrap_or(0);
        state.select(Some(selected));
    }
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} events ", model.view.label())),
        )
        .highlight_symbol("▸ ")
        .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD));
    frame.render_stateful_widget(list, sections[0], &mut state);

    let detail = model.selected_event_record().map_or_else(
        || {
            Text::from(vec![
                Line::from("No event selected."),
                Line::from("Use j/k or the arrow keys to inspect the live stream."),
            ])
        },
        |record| {
            Text::from(vec![
                field("Cursor", record.cursor.get().to_string()),
                field("Event", format_id(record.event_id.as_bytes())),
                field("Family", format!("{} ({})", record.family_name, record.family)),
                field("Schema", record.schema.to_string()),
                field("Attempt", record.attempt.to_string()),
                field("SHA-256", format_digest(&record.digest)),
                field("Frame bytes", record.byte_len.to_string()),
                Line::from(""),
                Line::styled(
                    "Bounded inert frame preview",
                    Style::default().fg(MUTED).add_modifier(Modifier::ITALIC),
                ),
                Line::from(record.preview.clone()),
            ])
        },
    );
    frame.render_widget(
        Paragraph::new(detail)
            .block(Block::default().borders(Borders::ALL).title(" Exact observation "))
            .wrap(Wrap { trim: false }),
        sections[1],
    );
}

fn render_terminal(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let (area, input_area) =
        if model.terminal.as_ref().is_some_and(crate::terminal::TerminalSession::uses_pipes) {
            let [output, input] =
                Layout::vertical([Constraint::Min(0), Constraint::Length(3)]).areas(area);
            (output, Some(input))
        } else {
            (area, None)
        };
    let title = model.terminal.as_ref().map_or_else(
        || " Terminal · detached ".to_owned(),
        |terminal| {
            format!(
                " Terminal · {} · input {} ",
                terminal.phase_label(),
                if terminal.capture_input() { "captured" } else { "released" }
            )
        },
    );
    let inner_height = usize::from(area.height.saturating_sub(2));
    let text = model.terminal.as_ref().map_or_else(
        || {
            Text::from(vec![
                Line::styled("No terminal is attached.", Style::default().fg(MUTED)),
                Line::from("Press a and enter a daemon-owned ProcessId to attach."),
            ])
        },
        |terminal| {
            Text::from(
                terminal
                    .visible_lines(inner_height)
                    .into_iter()
                    .map(Line::from)
                    .collect::<Vec<_>>(),
            )
        },
    );
    frame.render_widget(
        Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: false }),
        area,
    );
    if let Some((row, column)) =
        model.terminal.as_ref().and_then(crate::terminal::TerminalSession::cursor)
        && row < area.height.saturating_sub(2)
        && column < area.width.saturating_sub(2)
    {
        frame.set_cursor_position((area.x + 1 + column, area.y + 1 + row));
    }
    if let Some(input_area) = input_area
        && let Some(terminal) = &model.terminal
        && let Some((text, cursor, pending)) = terminal.line_input()
    {
        let draft = crate::input::composer::layout(
            text,
            cursor,
            None,
            usize::from(input_area.width.saturating_sub(2)),
        );
        let offset = draft.offset(input_area);
        let title = if pending {
            " Sending line · draft retained "
        } else {
            " Line input · Enter sends "
        };
        frame.render_widget(
            Paragraph::new(viewport(&draft.lines, offset, input_area.height.saturating_sub(2)))
                .block(Block::default().borders(Borders::ALL).title(title)),
            input_area,
        );
        if terminal.capture_input()
            && terminal.can_capture()
            && !pending
            && draft.column < usize::from(input_area.width.saturating_sub(2))
            && draft.row.saturating_sub(offset) < usize::from(input_area.height.saturating_sub(2))
        {
            frame.set_cursor_position((
                input_area.x + 1 + u16::try_from(draft.column).unwrap_or(0),
                input_area.y + 1 + u16::try_from(draft.row.saturating_sub(offset)).unwrap_or(0),
            ));
        }
    }
}

fn render_help(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let lines = crate::help::lines(area.width.saturating_sub(2));
    let maximum = lines.len().saturating_sub(usize::from(area.height.saturating_sub(2)).max(1));
    let scroll = model.help_scroll.min(maximum);
    frame.render_widget(
        Paragraph::new(viewport(&lines, scroll, area.height.saturating_sub(2))).block(
            Block::default().borders(Borders::ALL).title(" Keys · ↑/↓ · PgUp/PgDn · Home/End "),
        ),
        area,
    );
}

fn field(label: &'static str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<18}"), Style::default().fg(MUTED)),
        Span::raw(value),
    ])
}

fn short_id(bytes: &[u8; 16]) -> String {
    short_text(&format_id(bytes), 12)
}

fn short_text(text: &str, maximum: usize) -> String {
    if text.len() <= maximum {
        text.to_owned()
    } else {
        format!("{}…", &text[..text.floor_char_boundary(maximum)])
    }
}

/// Selects logical rows before handing content to Ratatui's terminal-sized coordinates.
fn viewport<'a>(lines: &[Line<'a>], offset: usize, height: u16) -> Vec<Line<'a>> {
    lines.iter().skip(offset).take(usize::from(height)).cloned().collect()
}

pub fn wrapped_lines(lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    chat::wrapped_lines(lines, width)
}
