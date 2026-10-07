//! Public activity projection and wrapping shared by rendering and mouse copy.

use super::{ACCENT, AppModel, BAD, GOOD, MUTED};
use crate::input::output::OutputRow;
use peritus_app_protocol::ProductActivityKind;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

pub fn transcript_rows(model: &AppModel, area: Rect) -> Vec<OutputRow> {
    let mut rows = Vec::new();
    if let Some(snapshot) = &model.chat.snapshot {
        let activities = model.interaction_activities(snapshot);
        let complete = model.interaction_history_is_complete(snapshot);
        let unavailable = model.unavailable_activity_prefix(snapshot);
        if unavailable != 0 {
            rows.extend(wrapped_rows(
                vec![Line::styled(
                    format!(
                        "{unavailable} earliest activities are unavailable because they predate complete history retention."
                    ),
                    Style::default().fg(MUTED),
                )],
                usize::from(area.width),
            ));
        }
        if let Some(window) = snapshot.activity_window().filter(|_| !complete) {
            rows.extend(wrapped_rows(
                vec![Line::styled(
                    format!(
                        "Loading complete conversation history; {} earlier activities are outside this live window.",
                        window.omitted()
                    ),
                    Style::default().fg(MUTED),
                )],
                usize::from(area.width),
            ));
            for activity in window.retained_errors() {
                append_activity_rows(&mut rows, model, activity, area.width);
            }
            if window.omitted_errors() != 0 {
                rows.extend(wrapped_rows(
                    vec![Line::styled(
                        format!("{} additional earlier errors are outside this live view.", window.omitted_errors()),
                        Style::default().fg(BAD),
                    )],
                    usize::from(area.width),
                ));
            }
        } else if snapshot.activity_window().is_none()
            && snapshot.activities().first().is_some_and(|first| first.sequence() > 1)
        {
            rows.extend(wrapped_rows(vec![Line::styled("Earlier activity is outside this bounded window. The durable conversation and trace remain available.", Style::default().fg(MUTED))], usize::from(area.width)));
        }
        for activity in activities {
            append_activity_rows(&mut rows, model, activity, area.width);
        }
        if let Some(activity) = snapshot
            .activity_window()
            .and_then(peritus_app_protocol::ProductActivityWindow::terminal_error)
        {
            append_activity_rows(&mut rows, model, activity, area.width);
        }
    } else {
        rows.extend(wrapped_rows(
            vec![
                Line::styled(
                    "What would you like to work on?",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Line::from(""),
                Line::from("Ask a question, explore an idea, or request a change."),
                Line::from(if model.direct_folder_chat().is_some() {
                    "/plan and /review are read-only. Ask for in-place work in /chat."
                } else {
                    "/plan and /review are read-only. /build starts checked delivery."
                }),
                Line::from("/model discovers models from your configured provider."),
                Line::from("/runs opens the existing run and candidate dashboard."),
            ],
            usize::from(area.width),
        ));
    }
    let offset =
        rows.len().saturating_sub(usize::from(area.height)).saturating_sub(model.chat.scroll);
    rows.into_iter().skip(offset).take(usize::from(area.height)).collect()
}

fn append_activity_rows(
    rows: &mut Vec<OutputRow>,
    model: &AppModel,
    activity: &peritus_app_protocol::ProductActivity,
    width: u16,
) {
    let thinking = activity.kind() == ProductActivityKind::Status
        && activity.detail() == "Provider thinking summary";
    if !model.chat.expanded && activity.kind() == ProductActivityKind::Status && !thinking {
        return;
    }
    if activity.kind() == ProductActivityKind::Tool {
        rows.extend(tool_rows(activity, model.chat.expanded, usize::from(width)));
        return;
    }
    let (label, color) = match activity.kind() {
        ProductActivityKind::User => ("You", ACCENT),
        ProductActivityKind::Assistant => {
            if activity.detail() == "Host recovery notice" {
                ("Recovery", ACCENT)
            } else {
                ("Peritus", GOOD)
            }
        }
        ProductActivityKind::Tool => ("Tool", MUTED),
        ProductActivityKind::Status => (if thinking { "Thinking" } else { "Status" }, MUTED),
        ProductActivityKind::Error => ("Error", BAD),
    };
    let mut lines =
        vec![Line::styled(label, Style::default().fg(color).add_modifier(Modifier::BOLD))];
    append_lines(&mut lines, activity.text());
    if u64::try_from(activity.text().len()).ok() != Some(activity.text_bytes()) {
        lines.push(Line::styled(
            format!("… text preview; {} total UTF-8 bytes …", activity.text_bytes()),
            Style::default().fg(MUTED),
        ));
    }
    if (model.chat.expanded || activity.kind() == ProductActivityKind::Error)
        && !activity.detail().is_empty()
    {
        append_lines(&mut lines, activity.detail());
        if u64::try_from(activity.detail().len()).ok() != Some(activity.detail_bytes()) {
            lines.push(Line::styled(
                format!("… detail preview; {} total UTF-8 bytes …", activity.detail_bytes()),
                Style::default().fg(MUTED),
            ));
        }
    }
    lines.push(Line::from(""));
    rows.extend(wrapped_rows(lines, usize::from(width)));
}

fn append_lines(lines: &mut Vec<Line<'static>>, text: &str) {
    let text = crate::sanitize::sanitize_display_text(text);
    lines.extend(text.lines().map(|line| Line::from(line.to_owned())));
}

#[cfg(test)]
pub(super) fn append_tool(
    lines: &mut Vec<Line<'static>>,
    activity: &peritus_app_protocol::ProductActivity,
    expanded: bool,
    width: usize,
) {
    lines.extend(tool_rows(activity, expanded, width).into_iter().map(|row| row.line));
}

fn tool_rows(
    activity: &peritus_app_protocol::ProductActivity,
    expanded: bool,
    width: usize,
) -> Vec<OutputRow> {
    let mut tool = vec![Line::styled(
        format!("• {}", crate::sanitize::sanitize_display_text(activity.text())),
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    )];
    let detail = crate::sanitize::sanitize_display_text(activity.detail());
    tool.extend(
        detail.lines().map(|line| Line::styled(format!("  │ {line}"), Style::default().fg(MUTED))),
    );
    let tool = wrapped_rows(tool, width);
    let mut rows = Vec::new();
    if !expanded && tool.len() > 7 {
        rows.extend(tool.iter().take(3).cloned());
        // The omitted middle is an explicit hard break, never hidden clipboard content.
        if let Some(last) = rows.last_mut() {
            last.hard_break = true;
        }
        rows.extend(wrapped_rows(
            vec![Line::styled(
                format!("  └ … {} more lines · /details", tool.len() - 6),
                Style::default().fg(MUTED),
            )],
            width,
        ));
        rows.extend(tool.iter().skip(tool.len() - 3).cloned());
    } else {
        rows.extend(tool);
    }
    rows.push(OutputRow { line: Line::from(""), hard_break: true });
    rows
}

pub fn wrapped_lines(lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    wrapped_rows(lines, width).into_iter().map(|row| row.line).collect()
}

fn wrapped_rows(lines: Vec<Line<'static>>, width: usize) -> Vec<OutputRow> {
    if width == 0 {
        return Vec::new();
    }
    let mut result = Vec::new();
    for line in lines {
        let style = line.style;
        if line.width() <= width {
            result.push(OutputRow { line, hard_break: true });
            continue;
        }
        let mut current = Line::default().style(style);
        let mut used = 0;
        for span in line.spans {
            for grapheme in span.styled_graphemes(style) {
                let cells = Span::raw(grapheme.symbol).width();
                if used != 0 && used.saturating_add(cells) > width {
                    result.push(OutputRow { line: current, hard_break: false });
                    current = Line::default().style(style);
                    used = 0;
                }
                if let Some(last) = current.spans.last_mut()
                    && last.style == span.style
                {
                    last.content.to_mut().push_str(grapheme.symbol);
                } else {
                    current.spans.push(Span::styled(grapheme.symbol.to_owned(), span.style));
                }
                used += cells;
            }
        }
        result.push(OutputRow { line: current, hard_break: true });
    }
    result
}
