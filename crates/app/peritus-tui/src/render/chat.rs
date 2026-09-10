//! Conversation-first terminal layout with a persistent multiline composer and scrollable text.

mod brief;
mod checkpoints;
mod compaction;
mod composer;
mod context;
mod doctor;
mod effort;
mod files;
mod goal;
mod images;
mod init;
mod memory;
mod permissions;
mod queue;
#[cfg(test)]
mod tests;
mod workbench;
use super::{ACCENT, BAD, GOOD, MUTED, WARN};
use crate::model::{AppModel, ConnectionStatus};
use peritus_app_protocol::ProductActivityKind;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

pub(super) fn draw(frame: &mut Frame<'_>, model: &AppModel) {
    let draft = composer::layout(
        &model.chat.buffer,
        model.chat.cursor,
        usize::from(frame.area().width.saturating_sub(2)),
    );
    let draft_lines = draft.lines.len().clamp(1, 6);
    let composer_height = u16::try_from(draft_lines).unwrap_or(6) + 2;
    let working_seconds = model.chat.working.elapsed_seconds();
    let regions = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(u16::from(working_seconds.is_some())),
        Constraint::Length(composer_height),
        Constraint::Length(2),
    ])
    .split(frame.area());
    let title = title(model);
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(title, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Line::styled(
                "Type / for commands · /model · /effort · PageUp/PageDown scroll · /details",
                Style::default().fg(MUTED),
            ),
        ]),
        regions[0],
    );
    if let Some(seconds) = working_seconds {
        frame.render_widget(
            Paragraph::new(format!("*working ({seconds}s)"))
                .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            regions[2],
        );
    }
    if model.chat.doctor.is_some() {
        doctor::draw(frame, regions[1], model);
    } else if model.chat.workbench.open {
        workbench::draw(frame, regions[1], model);
    } else if model.chat.effort_picker() {
        effort::draw(frame, regions[1], model);
    } else if model.chat.model_picker() {
        draw_models(frame, regions[1], model);
    } else {
        draw_transcript(frame, regions[1], model);
    }
    draw_composer(frame, regions[3], model, draft);
    draw_status(frame, regions[4], model);
}

fn title(model: &AppModel) -> String {
    let reviewing = model.chat.mode == peritus_app_protocol::ProductInteractionMode::Review;
    let provider_id = model
        .chat_providers()
        .map(|providers| if reviewing { providers.reviewer() } else { providers.writer() });
    let provider = model
        .product
        .as_ref()
        .and_then(|product| {
            product
                .launch
                .providers()
                .iter()
                .find(|provider| Some(provider.profile_id()) == provider_id)
        })
        .map_or("No available configured provider", crate::runtime::ProductProviderOption::label);
    let selected =
        if reviewing { model.chat.models.reviewer() } else { model.chat.models.writer() };
    let model_label = if selected.id().is_empty() { "configured model" } else { selected.id() };
    let connection = match &model.connection {
        ConnectionStatus::Online { .. } => "",
        ConnectionStatus::Connecting => " · connecting",
        ConnectionStatus::Disconnected(_) => " · disconnected",
    };
    let folder = model.direct_folder_chat();
    let workspace_mode = match folder {
        Some(true) => " · in-place folder",
        Some(false) => " · read-only folder",
        None => "",
    };
    crate::sanitize::sanitize_display_text(&format!(
        "Peritus · {} · {provider} · {model_label} · effort {}{workspace_mode}{connection}",
        model.chat.mode.label(),
        selected.effort().label()
    ))
}

fn draw_composer(
    frame: &mut Frame<'_>,
    area: Rect,
    model: &AppModel,
    draft: composer::DraftLayout,
) {
    let visible_rows = usize::from(area.height.saturating_sub(2)).max(1);
    let draft_offset = draft.row.saturating_sub(visible_rows - 1);
    let composer = Paragraph::new(
        draft
            .lines
            .into_iter()
            .skip(draft_offset)
            .take(visible_rows)
            .map(Line::from)
            .collect::<Vec<_>>(),
    )
    .block(
        Block::default().borders(Borders::ALL).border_style(Style::default().fg(ACCENT)).title(
            if model.chat.active() { " Message / steer active work " } else { " Message Peritus " },
        ),
    );
    frame.render_widget(composer, area);
    if !model.chat.model_picker()
        && !model.chat.effort_picker()
        && model.chat.doctor.is_none()
        && !model.chat.workbench.open
    {
        frame.set_cursor_position((
            area.x
                + 1
                + u16::try_from(draft.column).unwrap_or(u16::MAX).min(area.width.saturating_sub(3)),
            area.y
                + 1
                + u16::try_from(draft.row - draft_offset)
                    .unwrap_or(u16::MAX)
                    .min(area.height.saturating_sub(3)),
        ));
    }
}

fn draw_status(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let status = if !model.chat.expanded && model.chat.working.elapsed_seconds().is_some() {
        "Ctrl-C stops".to_owned()
    } else {
        model.chat.snapshot.as_ref().map_or_else(
            || "Ready · Enter sends · Shift-Enter newline · Ctrl-C exits".to_owned(),
            |snapshot| {
                format!(
                    "{} · input received {} / incorporated {} · Ctrl-C stops / exits",
                    snapshot.snapshot().status(),
                    snapshot.received(),
                    snapshot.incorporated()
                )
            },
        )
    };
    let notice = model.notice.as_ref().map_or("", |notice| notice.text.as_str());
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                crate::sanitize::sanitize_display_text(&status),
                Style::default().fg(MUTED),
            ),
            Line::styled(crate::sanitize::sanitize_display_text(notice), Style::default().fg(WARN)),
        ]),
        area,
    );
}

fn draw_transcript(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let commands = model.chat.matching_commands();
    if !commands.is_empty() {
        let mut state = ListState::default();
        state.select(Some(model.chat.command_selection.min(commands.len() - 1)));
        frame.render_stateful_widget(
            List::new(
                commands
                    .iter()
                    .map(|(name, description)| ListItem::new(format!("{name:<12} {description}"))),
            )
            .block(Block::default().title("Slash commands · arrows select · Tab completes"))
            .highlight_symbol("▸ ")
            .highlight_style(Style::default().fg(ACCENT)),
            area,
            &mut state,
        );
        return;
    }
    let mut lines = Vec::new();
    if let Some(snapshot) = &model.chat.snapshot {
        if snapshot.activities().first().is_some_and(|first| first.sequence() > 1) {
            lines.push(Line::styled("Earlier activity is outside this bounded window. The durable conversation and trace remain available.", Style::default().fg(MUTED)));
        }
        for activity in snapshot.activities() {
            if !model.chat.expanded && activity.kind() == ProductActivityKind::Status {
                continue;
            }
            if activity.kind() == ProductActivityKind::Tool {
                append_tool(&mut lines, activity, model.chat.expanded, usize::from(area.width));
                continue;
            }
            let (label, color) = match activity.kind() {
                ProductActivityKind::User => ("You", ACCENT),
                ProductActivityKind::Assistant => ("Peritus", GOOD),
                ProductActivityKind::Tool => ("Tool", MUTED),
                ProductActivityKind::Status => ("Status", MUTED),
                ProductActivityKind::Error => ("Error", BAD),
            };
            lines
                .push(Line::styled(label, Style::default().fg(color).add_modifier(Modifier::BOLD)));
            append_lines(&mut lines, activity.text());
            if model.chat.expanded && !activity.detail().is_empty() {
                append_lines(&mut lines, activity.detail());
            }
            lines.push(Line::from(""));
        }
    } else {
        lines.extend([
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
        ]);
    }
    let lines = wrapped_lines(lines, usize::from(area.width));
    let offset =
        lines.len().saturating_sub(usize::from(area.height)).saturating_sub(model.chat.scroll);
    frame.render_widget(
        Paragraph::new(Text::from(
            lines.into_iter().skip(offset).take(usize::from(area.height)).collect::<Vec<_>>(),
        )),
        area,
    );
}

fn append_lines(lines: &mut Vec<Line<'static>>, text: &str) {
    let text = crate::sanitize::sanitize_display_text(text);
    lines.extend(text.lines().map(|line| Line::from(line.to_owned())));
}

fn append_tool(
    lines: &mut Vec<Line<'static>>,
    activity: &peritus_app_protocol::ProductActivity,
    expanded: bool,
    width: usize,
) {
    let mut tool = vec![Line::styled(
        format!("• {}", crate::sanitize::sanitize_display_text(activity.text())),
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
    )];
    let detail = crate::sanitize::sanitize_display_text(activity.detail());
    tool.extend(
        detail.lines().map(|line| Line::styled(format!("  │ {line}"), Style::default().fg(MUTED))),
    );
    let tool = wrapped_lines(tool, width);
    if !expanded && tool.len() > 7 {
        lines.extend(tool.iter().take(3).cloned());
        lines.push(Line::styled(
            format!("  └ … {} more lines · /details", tool.len() - 6),
            Style::default().fg(MUTED),
        ));
        lines.extend(tool.iter().skip(tool.len() - 3).cloned());
    } else {
        lines.extend(tool);
    }
    lines.push(Line::from(""));
}

pub(super) fn wrapped_lines(lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let mut result = Vec::new();
    for line in lines {
        let style = line.style;
        if line.width() <= width {
            result.push(line);
            continue;
        }
        let mut current = Line::default().style(style);
        let mut used = 0;
        for span in line.spans {
            let mut start = 0;
            for (index, character) in span.content.char_indices() {
                let mut bytes = [0; 4];
                let cells = Span::raw(&*character.encode_utf8(&mut bytes)).width();
                if used + cells > width {
                    if start < index {
                        current
                            .spans
                            .push(Span::styled(span.content[start..index].to_owned(), span.style));
                    }
                    result.push(current);
                    current = Line::default().style(style);
                    used = 0;
                    start = index;
                }
                used += cells;
            }
            if start < span.content.len() {
                current.spans.push(Span::styled(span.content[start..].to_owned(), span.style));
            }
        }
        result.push(current);
    }
    result
}

fn draw_models(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let regions =
        Layout::vertical([Constraint::Length(4), Constraint::Min(1), Constraint::Length(3)])
            .split(area);
    let Some(catalog) = &model.chat.catalog else {
        frame.render_widget(Paragraph::new("Querying configured provider model catalog…\ne selects effort · Escape closes; no inference request is sent."), area);
        return;
    };
    let provenance = if catalog.cached() {
        "cached provider catalog"
    } else if catalog.fetched_unix_seconds() == 0 {
        "discovery unavailable"
    } else {
        "fresh provider catalog"
    };
    frame.render_widget(
        Paragraph::new(format!(
            "Model for {} · effort {} · {provenance}\nConfigured: {} · fetched at Unix {}\n{}",
            model.chat.model_role.label(),
            model.chat.model_role.choice(&model.chat.models).effort().label(),
            catalog.configured(),
            catalog.fetched_unix_seconds(),
            catalog.error()
        ))
        .wrap(Wrap { trim: false }),
        regions[0],
    );
    let items = catalog
        .models()
        .iter()
        .map(|entry| {
            ListItem::new(format!(
                "{} · {} · tools {}",
                entry.id(),
                entry.label(),
                match entry.tools() {
                    Some(true) => "advertised",
                    Some(false) => "unsupported",
                    None => "unknown",
                }
            ))
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default();
    if !items.is_empty() {
        state.select(Some(model.chat.model_selection.min(items.len() - 1)));
    }
    frame.render_stateful_widget(
        List::new(items).highlight_symbol("▸ ").highlight_style(Style::default().fg(ACCENT)),
        regions[1],
        &mut state,
    );
    frame.render_widget(Paragraph::new("Enter selects · e selects effort · Tab changes role · r refreshes · Esc closes\nListing is not capability verification. Manual fallback: /model [role] manual ID").wrap(Wrap { trim: false }), regions[2]);
}
