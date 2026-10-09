//! Dashboard, diff, and review presentation for daemon-owned coding runs.

#[cfg(test)]
use peritus_app_protocol::ProductRunPhase;
use peritus_app_protocol::ProductRunSnapshot;
#[cfg(test)]
use peritus_run_settlement::CandidateStage;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use super::{ACCENT, MUTED, short_text};
use crate::model::AppModel;

mod detail;
mod preview;
mod structured_review;

#[cfg(test)]
use detail::product_state;
use detail::{
    conversation_text, empty_detail, inspect_text, phase_symbol, render_run_text, run_detail, safe,
};

pub(super) fn dashboard(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let areas = dashboard_areas(area);
    let Some(product) = &model.product else { return };
    let composer = Text::from(vec![
        Line::styled(
            "What should Peritus build?",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Line::from("Press Esc, then use /build <request> in Conversation to start checked work."),
        Line::from(""),
        Line::from(vec![
            Span::styled("Workspace  ", Style::default().fg(MUTED)),
            Span::raw(safe(product.launch.workspace_label())),
        ]),
        Line::from(format!(
            "Writer [{}]  Reviewer [{}]  Fixer [{}]",
            product.writer_label(),
            product.reviewer_label(),
            product.fixer_label()
        )),
    ]);
    frame.render_widget(
        Paragraph::new(composer)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(ACCENT))
                    .title(" Checked coding runs · /build starts · w/e/f choose providers "),
            )
            .wrap(Wrap { trim: false }),
        areas.composer,
    );

    let items = if product.runs.is_empty() {
        vec![ListItem::new(Line::styled("No coding runs yet", Style::default().fg(MUTED)))]
    } else {
        product
            .runs
            .iter()
            .map(|run| {
                ListItem::new(format!(
                    "{}  {}",
                    phase_symbol(run.phase()),
                    short_text(run.task(), 42)
                ))
            })
            .collect()
    };
    let mut state = ListState::default();
    if !product.runs.is_empty() {
        state.select(Some(product.selected));
    }
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).title(" Runs "))
            .highlight_symbol("▸ ")
            .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)),
        areas.runs,
        &mut state,
    );
    let detail = product.selected_run().map_or_else(empty_detail, |run| {
        run_detail(
            run,
            product.selected_settlement(),
            product.confirmation.as_ref().map(|value| value.warning.as_str()),
        )
    });
    let detail =
        super::chat::wrapped_lines(detail.lines, usize::from(areas.detail.width.saturating_sub(2)));
    let maximum = content_scroll_limit(detail.len(), areas.detail);
    let controls = product.selected_run().map_or_else(|| "Progress".to_owned(), control_title);
    frame.render_widget(
        Paragraph::new(super::viewport(
            &detail,
            product.detail_scroll.min(maximum),
            areas.detail.height.saturating_sub(2),
        ))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {controls} · PgUp/PgDn · Home/End · i inspect ")),
        ),
        areas.detail,
    );
    frame.render_widget(
        Paragraph::new(conversation_text(product.selected_conversation()))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(ACCENT))
                    .title(" Conversation · Enter/m message this run "),
            )
            .wrap(Wrap { trim: false }),
        areas.conversation,
    );
}

fn control_title(run: &ProductRunSnapshot) -> String {
    let controls = run.operation().legal_controls();
    let mut actions = Vec::new();
    for (allowed, label) in [
        (controls.cancel(), "x cancel"),
        (controls.retry(), "r exact retry"),
        (controls.accept(), "a accept"),
        (controls.commit(), "c commit"),
        (controls.export(), "p export"),
        (controls.discard(), "D discard"),
        (controls.acknowledge(), "u acknowledge uncertainty"),
    ] {
        if allowed {
            actions.push(label);
        }
    }
    if actions.is_empty() {
        "Progress · no operation controls available".to_owned()
    } else {
        format!("Progress · {}", actions.join(" · "))
    }
}

#[derive(Clone, Copy)]
struct DashboardAreas {
    composer: Rect,
    runs: Rect,
    detail: Rect,
    conversation: Rect,
}

fn dashboard_areas(area: Rect) -> DashboardAreas {
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(6)])
        .split(area);
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(regions[1]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(43), Constraint::Percentage(57)])
        .split(columns[1]);
    DashboardAreas {
        composer: regions[0],
        runs: columns[0],
        detail: right[0],
        conversation: right[1],
    }
}

fn detail_scroll_limit(detail: &Text<'static>, area: Rect) -> usize {
    let lines =
        super::chat::wrapped_lines(detail.lines.clone(), usize::from(area.width.saturating_sub(2)))
            .len();
    content_scroll_limit(lines, area)
}

pub(super) fn diff(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    if model.product.as_ref().is_some_and(|product| product.review.raw_stream) {
        structured_review::render_raw_stream(frame, area, model);
        return;
    }
    if let Some(review) = model.product.as_ref().map(|product| &product.review)
        && review.page.is_some()
        && !review.raw
    {
        structured_review::render(frame, area, model);
        return;
    }
    render_run_text(
        frame,
        area,
        model,
        " Raw candidate diff · t structured · r refresh ",
        inspect_text,
        "No diff is available for the selected run yet.",
    );
}

pub(super) fn review(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    render_run_text(
        frame,
        area,
        model,
        " Independent review and checks ",
        |run| if run.review().is_empty() { run.gates() } else { run.review() }.to_owned(),
        "No review is available for the selected run yet.",
    );
}

pub(super) fn preview(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    preview::render(frame, area, model);
}

pub(super) fn scroll_limit(model: &AppModel, area: Rect) -> usize {
    if model.view == crate::model::View::Diff
        && model.product.as_ref().is_some_and(|product| product.review.raw_stream)
    {
        return structured_review::raw_stream_scroll_limit(model, area);
    }
    if model.view == crate::model::View::Diff
        && model
            .product
            .as_ref()
            .is_some_and(|product| product.review.page.is_some() && !product.review.raw)
    {
        return structured_review::scroll_limit(model, area);
    }
    let lines = match model.view {
        crate::model::View::Runs => {
            let Some(product) = &model.product else { return 0 };
            let detail = product.selected_run().map_or_else(empty_detail, |run| {
                run_detail(
                    run,
                    product.selected_settlement(),
                    product.confirmation.as_ref().map(|value| value.warning.as_str()),
                )
            });
            return detail_scroll_limit(&detail, dashboard_areas(area).detail);
        }
        crate::model::View::Preview => preview::content(model, area.width),
        crate::model::View::Diff => detail::run_text_lines(model, area.width, inspect_text, ""),
        crate::model::View::Review => detail::run_text_lines(
            model,
            area.width,
            |run| if run.review().is_empty() { run.gates() } else { run.review() }.to_owned(),
            "",
        ),
        _ => Vec::new(),
    };
    content_scroll_limit(lines.len(), area)
}

pub(super) fn scroll_page(area: Rect) -> usize {
    usize::from(dashboard_areas(area).detail.height.saturating_sub(2).max(1))
}

fn content_scroll_limit(lines: usize, area: Rect) -> usize {
    lines.saturating_sub(usize::from(area.height.saturating_sub(2)))
}

#[cfg(test)]
mod tests;
