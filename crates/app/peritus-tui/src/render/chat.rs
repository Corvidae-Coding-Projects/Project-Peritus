//! Conversation-first terminal layout with a persistent multiline composer and scrollable text.

mod brief;
mod checkpoints;
mod compaction;
mod context;
pub(super) mod doctor;
mod effort;
mod files;
mod goal;
mod images;
mod init;
pub(super) mod inspector;
pub(super) mod library;
mod memory;
mod permissions;
mod queue;
mod status;
mod transcript;
#[cfg(test)]
use transcript::append_tool;
pub use transcript::transcript_rows;
pub(super) use transcript::wrapped_lines;
#[cfg(test)]
mod tests;
mod workbench;
use super::{ACCENT, BAD, GOOD, MUTED, WARN};
use crate::{
    input::composer,
    model::{AppModel, ConnectionStatus},
};
#[cfg(test)]
use peritus_app_protocol::ProductActivityKind;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

pub(super) fn draw(frame: &mut Frame<'_>, model: &AppModel) {
    let draft = composer::layout(
        &model.chat.buffer,
        model.chat.cursor,
        model.chat.selection().as_ref(),
        usize::from(frame.area().width.saturating_sub(2)),
    );
    let working_seconds = model.chat.working.elapsed_seconds();
    let regions = composer::regions(frame.area(), draft.lines.len(), working_seconds.is_some());
    let title = title(model);
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(title, Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
            Line::styled(
                if model.chat.selecting_output() {
                    "Paste/F2/Esc returns · SELECT: drag, then terminal Copy · work continues"
                } else {
                    "/effort · Drag text · Right-click Copy · Wheel scrolls · F2 · Type /"
                },
                Style::default().fg(MUTED),
            ),
        ]),
        regions[0],
    );
    if let Some(seconds) = working_seconds {
        let progress = model.chat.snapshot.as_ref().map_or_else(
            || format!("elapsed {seconds}s"),
            |snapshot| snapshot.snapshot().status().to_owned(),
        );
        frame.render_widget(
            Paragraph::new(crate::sanitize::sanitize_display_text(&format!(
                "*working · {progress}"
            )))
            .wrap(Wrap { trim: false })
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
    status::draw(frame, regions[4], model);
    if let Some(selection) = &model.chat.output_selection
        && selection.area == regions[1]
        && let Some(menu) = selection.menu
    {
        frame.render_widget(ratatui::widgets::Clear, menu);
        frame.render_widget(
            Paragraph::new(" Copy ")
                .style(Style::default().fg(ACCENT))
                .block(Block::default().borders(Borders::ALL)),
            menu,
        );
    }
}

pub(super) fn status_scroll_metrics(model: &AppModel, viewport: Rect) -> (usize, usize) {
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
    status::scroll_metrics(model, regions[4])
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
        "Peritus{connection} · {} · {provider} · {model_label} · effort {}{workspace_mode}",
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
    let draft_offset = draft.offset(area);
    let composer = Paragraph::new(
        draft.lines.into_iter().skip(draft_offset).take(visible_rows).collect::<Vec<_>>(),
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
    let lines = model
        .chat
        .output_selection
        .as_ref()
        .filter(|selection| selection.area == area)
        .map_or_else(
            || transcript_rows(model, area).into_iter().map(|row| row.line).collect(),
            crate::input::output::OutputSelection::highlighted_lines,
        );
    frame.render_widget(Paragraph::new(Text::from(lines)), area);
}

pub fn transcript_area(model: &AppModel, viewport: Rect) -> Rect {
    let draft = composer::layout(
        &model.chat.buffer,
        model.chat.cursor,
        model.chat.selection().as_ref(),
        usize::from(viewport.width.saturating_sub(2)),
    );
    composer::regions(viewport, draft.lines.len(), model.chat.working.elapsed_seconds().is_some())
        [1]
}

fn draw_models(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let regions =
        Layout::vertical([Constraint::Length(4), Constraint::Min(1), Constraint::Length(3)])
            .split(area);
    let Some(catalog) = &model.chat.catalog else {
        let status = if model.model_discovery_pending() {
            "Querying configured provider model catalog…"
        } else {
            "Model catalog unavailable. r retries discovery."
        };
        frame.render_widget(
            Paragraph::new(format!(
                "{status}\ne selects effort · Escape closes; no inference request is sent."
            ))
            .wrap(Wrap { trim: false }),
            area,
        );
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
