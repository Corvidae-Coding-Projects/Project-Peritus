//! Dashboard, diff, and review presentation for daemon-owned coding runs.

#[cfg(test)]
use peritus_app_protocol::{ProductRunPhase, ProductRunSnapshot};
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
    let regions = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(7), Constraint::Min(6)])
        .split(area);
    let Some(product) = &model.product else { return };
    let composer = Text::from(vec![
        Line::styled(
            "What should Peritus build?",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Line::from("Press n to describe a task and begin the writer → reviewer → fixer loop."),
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
                    .title(" New coding run · n start · w/e/f choose providers "),
            )
            .wrap(Wrap { trim: false }),
        regions[0],
    );

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(regions[1]);
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
        columns[0],
        &mut state,
    );
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(43), Constraint::Percentage(57)])
        .split(columns[1]);
    let detail = product.selected_run().map_or_else(empty_detail, |run| {
        run_detail(
            run,
            product.selected_settlement(),
            product.confirmation.as_ref().map(|value| value.warning.as_str()),
        )
    });
    frame.render_widget(
        Paragraph::new(detail)
            .block(Block::default().borders(Borders::ALL).title(
                " Progress · i inspect · v run · a accept · c commit · p export · D discard ",
            ))
            .wrap(Wrap { trim: false }),
        right[0],
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
        right[1],
    );
}

pub(super) fn diff(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
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

#[cfg(test)]
mod tests;
