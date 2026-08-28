//! Dashboard, diff, and review presentation for daemon-owned coding runs.

use peritus_app_protocol::{ProductRunPhase, ProductRunSnapshot};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use super::{ACCENT, BAD, GOOD, MUTED, WARN, field, short_text};
use crate::model::AppModel;

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
    let detail = product.selected_run().map_or_else(empty_detail, run_detail);
    frame.render_widget(
        Paragraph::new(detail)
            .block(Block::default().borders(Borders::ALL).title(" Progress · x cancel · r retry "))
            .wrap(Wrap { trim: false }),
        columns[1],
    );
}

pub(super) fn diff(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    render_run_text(
        frame,
        area,
        model,
        " Current managed-worktree diff ",
        |run| run.diff(),
        "No diff is available for the selected run yet.",
    );
}

pub(super) fn review(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    render_run_text(
        frame,
        area,
        model,
        " Independent review and checks ",
        |run| {
            if run.review().is_empty() { run.gates() } else { run.review() }
        },
        "No review is available for the selected run yet.",
    );
}

fn render_run_text(
    frame: &mut Frame<'_>,
    area: Rect,
    model: &AppModel,
    title: &str,
    select: impl FnOnce(&ProductRunSnapshot) -> &str,
    empty: &str,
) {
    let text = model.product.as_ref().and_then(|product| product.selected_run()).map_or_else(
        || empty.to_owned(),
        |run| {
            let value = select(run);
            if value.is_empty() { empty.to_owned() } else { safe(value) }
        },
    );
    frame.render_widget(
        Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn run_detail(run: &ProductRunSnapshot) -> Text<'static> {
    Text::from(vec![
        Line::styled(
            phase_line(run.phase()),
            phase_style(run.phase()).add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::from(timeline(run.phase())),
        Line::from(""),
        field("Current work", safe(run.status())),
        field("Cycle", run.cycle().to_string()),
        field("Task", safe(run.task())),
        Line::from(""),
        Line::styled(
            if run.summary().is_empty() {
                "The daemon will report each completed effect boundary here.".to_owned()
            } else {
                safe(run.summary())
            },
            Style::default().fg(MUTED),
        ),
    ])
}

fn empty_detail() -> Text<'static> {
    Text::from(vec![
        Line::from("Ready."),
        Line::from("Press n and describe a useful coding outcome."),
    ])
}

const fn phase_symbol(phase: ProductRunPhase) -> &'static str {
    match phase {
        ProductRunPhase::Queued => "○ Queued",
        ProductRunPhase::Writing => "● Writing",
        ProductRunPhase::Checking => "● Checking",
        ProductRunPhase::Reviewing => "● Reviewing",
        ProductRunPhase::Fixing => "● Fixing",
        ProductRunPhase::Verifying => "● Verifying",
        ProductRunPhase::Complete => "✓ Complete",
        ProductRunPhase::Failed => "✗ Failed",
        ProductRunPhase::Cancelled => "■ Cancelled",
        ProductRunPhase::RecoveryRequired => "! Recover",
    }
}

fn phase_line(phase: ProductRunPhase) -> String {
    phase_symbol(phase).to_owned()
}

fn phase_style(phase: ProductRunPhase) -> Style {
    match phase {
        ProductRunPhase::Complete => Style::default().fg(GOOD),
        ProductRunPhase::Failed | ProductRunPhase::Cancelled => Style::default().fg(BAD),
        ProductRunPhase::RecoveryRequired => Style::default().fg(WARN),
        _ => Style::default().fg(ACCENT),
    }
}

fn timeline(phase: ProductRunPhase) -> String {
    let active = match phase {
        ProductRunPhase::Queued => 0,
        ProductRunPhase::Writing => 1,
        ProductRunPhase::Checking => 2,
        ProductRunPhase::Reviewing => 3,
        ProductRunPhase::Fixing => 4,
        ProductRunPhase::Verifying => 5,
        ProductRunPhase::Complete => 6,
        ProductRunPhase::Failed
        | ProductRunPhase::Cancelled
        | ProductRunPhase::RecoveryRequired => 7,
    };
    ["Understand", "Write", "Check", "Review", "Fix", "Verify", "Complete"]
        .iter()
        .enumerate()
        .map(
            |(index, label)| {
                if index == active { format!("[{label}]") } else { (*label).to_owned() }
            },
        )
        .collect::<Vec<_>>()
        .join(" → ")
}

fn safe(value: &str) -> String {
    value
        .chars()
        .filter(|character| *character == '\n' || *character == '\t' || !character.is_control())
        .collect()
}
