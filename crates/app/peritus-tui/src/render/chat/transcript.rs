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
        if snapshot.activities().first().is_some_and(|first| first.sequence() > 1) {
            rows.extend(wrapped_rows(vec![Line::styled("Earlier activity is outside this bounded window. The durable conversation and trace remain available.", Style::default().fg(MUTED))], usize::from(area.width)));
        }
        let mut activities = snapshot.activities().iter().peekable();
        while let Some(activity) = activities.next() {
            let thinking = activity.kind() == ProductActivityKind::Status
                && activity.detail() == "Provider thinking summary";
            if activity.kind() == ProductActivityKind::Tool {
                let mut body = activity.detail().to_owned();
                let continuation =
                    format!("{} · continued", activity.text().trim_end_matches(" · continued"));
                while activities.peek().is_some_and(|next| {
                    next.kind() == ProductActivityKind::Tool && next.text() == continuation
                }) {
                    body.push_str(activities.next().expect("peeked activity").detail());
                }
                rows.extend(tool_rows(model, activity, &body, usize::from(area.width)));
                continue;
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
                ProductActivityKind::Status => {
                    (if thinking { "Thinking" } else { "Progress" }, ACCENT)
                }
                ProductActivityKind::Error => ("Error", BAD),
            };
            let mut lines =
                vec![Line::styled(label, Style::default().fg(color).add_modifier(Modifier::BOLD))];
            if activity.kind() == ProductActivityKind::Assistant || thinking {
                lines.extend(super::markdown::lines(model, activity, usize::from(area.width)));
            } else {
                append_lines(&mut lines, activity.text());
            }
            if (model.chat.expanded || activity.kind() == ProductActivityKind::Error)
                && !activity.detail().is_empty()
            {
                append_lines(&mut lines, activity.detail());
            }
            lines.push(Line::from(""));
            let width = usize::from(area.width);
            rows.extend(wrapped_rows(
                lines,
                if activity.kind() == ProductActivityKind::Assistant || thinking {
                    width.min(110)
                } else {
                    width
                },
            ));
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
                    "/design guides a saved design. /plan and /review are read-only. Ask for in-place work in /chat."
                } else {
                    "/design guides a saved design. /plan and /review are read-only. /build starts checked delivery."
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

fn append_lines(lines: &mut Vec<Line<'static>>, text: &str) {
    let text = crate::sanitize::sanitize_display_text(text);
    lines.extend(text.lines().map(|line| Line::from(line.to_owned())));
}

fn tool_rows(
    model: &AppModel,
    activity: &peritus_app_protocol::ProductActivity,
    body: &str,
    width: usize,
) -> Vec<OutputRow> {
    let text = crate::sanitize::sanitize_display_text(activity.text());
    let failed = text.starts_with("Failed ");
    let pending = text.starts_with("Calling ");
    let changed = ["Updated ", "Wrote ", "Removed "].iter().any(|prefix| text.starts_with(prefix));
    let color = if failed {
        BAD
    } else if changed {
        GOOD
    } else {
        Color::White
    };
    let mut tool = vec![Line::styled(
        format!(
            "{} {}",
            if failed {
                "!"
            } else if pending {
                "▶"
            } else {
                "✓"
            },
            tool_label(&text),
        ),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )];
    let detail = crate::sanitize::sanitize_display_text(body);
    let rendered = serde_json::from_str::<serde_json::Value>(&detail)
        .ok()
        .map(|value| peritus_product_state::tool_output::render(&value))
        .unwrap_or(detail);
    let document = text.starts_with("Read document from saved context")
        || ((text.contains("workspace_read ") || text.contains("skill_read "))
            && text.split_whitespace().any(|token| {
                token.trim_matches(['\'', '"']).to_ascii_lowercase().ends_with(".md")
            }));
    if document && !model.chat.expanded {
        let prose = rendered
            .lines()
            .map(|line| {
                if text.contains("workspace_read ") {
                    line.split_once(": ")
                        .filter(|(number, _)| number.chars().all(|ch| ch.is_ascii_digit()))
                        .map_or(line, |(_, body)| body)
                } else {
                    line
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let readable_width = width.min(110);
        tool.push(Line::from(""));
        tool.extend(super::markdown::cached(model, activity.sequence(), &prose, readable_width));
        let mut rows = wrapped_rows(tool, readable_width);
        rows.push(OutputRow { line: Line::from(""), hard_break: true });
        return rows;
    }
    let source_path = text
        .split("path=")
        .nth(1)
        .and_then(|rest| rest.split_whitespace().next())
        .unwrap_or_default()
        .trim_matches(['\'', '"']);
    let highlighted = if changed || text.contains("workspace_read ") {
        cached_source(model, activity.sequence(), &rendered, source_path, changed)
    } else {
        rendered.lines().map(|line| Line::raw(line.to_owned())).collect()
    };
    tool.extend(highlighted.into_iter().map(|mut styled| {
        let line = styled.to_string();
        let color = if changed && line.starts_with('+') {
            GOOD
        } else if (changed && line.starts_with('-')) || line.starts_with("stderr:") {
            BAD
        } else if line.starts_with("@@") {
            ACCENT
        } else {
            Color::Reset
        };
        styled.spans.insert(0, Span::raw("  │ "));
        if color != Color::Reset {
            styled.style = styled.style.fg(color);
        }
        styled
    }));
    let mut rows = wrapped_rows(tool, width);
    rows.push(OutputRow { line: Line::from(""), hard_break: true });
    rows
}

fn cached_source(
    model: &AppModel,
    sequence: u64,
    source: &str,
    path: &str,
    diff: bool,
) -> Vec<Line<'static>> {
    let key = format!("syntax:{path}:{diff}\0{source}");
    let mut cache = model.chat.formatted_messages.borrow_mut();
    if let Some(entry) = cache.get(&sequence).filter(|entry| entry.source == key) {
        return entry.lines.clone();
    }
    let theme = model.chat.convenience.preferences.theme;
    let lines = if diff {
        crate::highlight::diff(source, theme)
    } else {
        crate::highlight::code(source, path, theme)
    };
    cache.insert(
        sequence,
        crate::model::chat::FormattedMessage { source: key, width: 0, lines: lines.clone() },
    );
    lines
}

fn tool_label(text: &str) -> String {
    let prefixes = [
        ("Called workspace_read ", "Read "),
        ("Calling workspace_read ", "Reading "),
        ("Called skill_read ", "Read skill "),
        ("Calling skill_read ", "Reading skill "),
        ("Called workspace_search ", "Searched "),
        ("Calling workspace_search ", "Searching "),
        ("Called workspace_list ", "Listed "),
        ("Calling workspace_list ", "Listing "),
        ("Called context_read ", "Read saved context "),
        ("Calling context_read ", "Reading saved context "),
        ("Called run_pipeline", "Handed work to the coding agent"),
    ];
    for (prefix, replacement) in prefixes {
        if let Some(rest) = text.strip_prefix(prefix) {
            return format!(
                "{replacement}{}",
                rest.replace("path=", "")
                    .replace("query=", "for ")
                    .replace("skill=", "· ")
                    .replace(" · continued", "")
            );
        }
    }
    text.to_owned()
}

pub fn wrapped_lines(lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    wrapped_rows(lines, width).into_iter().map(|row| row.line).collect()
}

fn wrapped_rows(lines: Vec<Line<'static>>, width: usize) -> Vec<OutputRow> {
    if width == 0 {
        return Vec::new();
    }
    let mut result = Vec::new();
    for line in lines.into_iter().flat_map(split_logical_line) {
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
                if cells > width {
                    continue;
                }
                if used + cells > width {
                    let text = current.to_string();
                    let boundary = text.char_indices().rev().find_map(|(index, ch)| {
                        (ch.is_whitespace() && index > 0).then_some(index + ch.len_utf8())
                    });
                    if let Some(boundary) = boundary {
                        let (head, tail) = split_at_byte(current, boundary);
                        result.push(OutputRow { line: head, hard_break: false });
                        current = tail;
                        used = current.width();
                    } else {
                        result.push(OutputRow { line: current, hard_break: false });
                        current = Line::default().style(style);
                        used = 0;
                    }
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

// Keep the whitespace at a soft wrap so copying styled output preserves the original text.
fn split_at_byte(line: Line<'static>, offset: usize) -> (Line<'static>, Line<'static>) {
    let mut head = Line::default().style(line.style);
    let mut tail = Line::default().style(line.style);
    let mut remaining = offset;
    for span in line.spans {
        let split = remaining.min(span.content.len());
        if split > 0 {
            head.spans.push(Span::styled(span.content[..split].to_owned(), span.style));
        }
        if split < span.content.len() {
            tail.spans.push(Span::styled(span.content[split..].to_owned(), span.style));
        }
        remaining = remaining.saturating_sub(split);
    }
    (head, tail)
}

fn split_logical_line(line: Line<'static>) -> Vec<Line<'static>> {
    if !line.spans.iter().any(|span| span.content.contains('\n')) {
        return vec![line];
    }
    let mut lines = Vec::new();
    let mut current = Line::default().style(line.style);
    for span in line.spans {
        for (index, part) in span.content.split('\n').enumerate() {
            if index > 0 {
                lines.push(current);
                current = Line::default().style(line.style);
            }
            current.spans.push(Span::styled(part.to_owned(), span.style));
        }
    }
    lines.push(current);
    lines
}
