//! Dashboard, diff, and review presentation for daemon-owned coding runs.

use peritus_app_protocol::{
    ProductConversationRole, ProductRunConversation, ProductRunPhase, ProductRunSnapshot,
};
use peritus_run_settlement::{
    CandidateStage, EvidenceStatus, QualificationEvidence, RunSettlement,
};
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
    render_run_text(
        frame,
        area,
        model,
        " Current managed-worktree diff ",
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

fn render_run_text(
    frame: &mut Frame<'_>,
    area: Rect,
    model: &AppModel,
    title: &str,
    select: impl FnOnce(&ProductRunSnapshot) -> String,
    empty: &str,
) {
    let text = model.product.as_ref().and_then(|product| product.selected_run()).map_or_else(
        || empty.to_owned(),
        |run| {
            let value = select(run);
            if value.is_empty() { empty.to_owned() } else { safe(&value) }
        },
    );
    frame.render_widget(
        Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(title))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn run_detail(
    run: &ProductRunSnapshot,
    settlement: Option<&RunSettlement>,
    confirmation: Option<&str>,
) -> Text<'static> {
    let mut lines = vec![
        Line::styled(product_state(run), product_state_style(run).add_modifier(Modifier::BOLD)),
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
    ];
    if let Some(deliverable) = run.deliverable() {
        lines.push(Line::from(""));
        lines.push(Line::styled(
            "Deliverable handoff",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ));
        lines.push(field("Managed path", safe(deliverable.workspace_path())));
        lines.push(field(
            "Qualification",
            qualification_name(deliverable.qualification()).to_owned(),
        ));
        if let Some(checkpoint) = settlement.and_then(RunSettlement::checkpoint) {
            lines.push(field("Checks", evidence_name(checkpoint.gates()).to_owned()));
            lines.push(field("Requirements", evidence_name(checkpoint.obligations()).to_owned()));
            lines.push(field("Review", evidence_name(checkpoint.review()).to_owned()));
        }
        lines.push(field("Changed files", deliverable.changed_paths().len().to_string()));
        lines.push(field("Run", safe(deliverable.run_instructions())));
        let state = if deliverable.discarded() {
            "discarded".to_owned()
        } else if !deliverable.commit_revision().is_empty() {
            format!("committed {}", short_text(deliverable.commit_revision(), 12))
        } else if deliverable.accepted() {
            "accepted".to_owned()
        } else {
            "ready for inspection".to_owned()
        };
        lines.push(field("State", state));
        if !deliverable.export_path().is_empty() {
            lines.push(field("Export", safe(deliverable.export_path())));
        }
    }
    if let Some(warning) = confirmation {
        lines.push(Line::from(""));
        lines.push(Line::styled(
            safe(warning),
            Style::default().fg(WARN).add_modifier(Modifier::BOLD),
        ));
    }
    Text::from(lines)
}

fn inspect_text(run: &ProductRunSnapshot) -> String {
    let Some(deliverable) = run.deliverable() else { return run.diff().to_owned() };
    let paths = if deliverable.changed_paths().is_empty() {
        "(no workspace paths; see successful external commands)".to_owned()
    } else {
        deliverable.changed_paths().join("\n")
    };
    let commands = if deliverable.successful_commands().is_empty() {
        "(none recorded)".to_owned()
    } else {
        deliverable.successful_commands().join("\n")
    };
    format!(
        "Workspace\n{}\n\nExact candidate paths\n{}\n\nSuccessful commands\n{}\n\nRun instructions\n{}\n\nDiff\n{}",
        deliverable.workspace_path(),
        paths,
        commands,
        deliverable.run_instructions(),
        run.diff(),
    )
}

fn product_state(run: &ProductRunSnapshot) -> String {
    match (run.phase(), run.deliverable()) {
        (ProductRunPhase::Complete, _) => "Accepted".to_owned(),
        (ProductRunPhase::WaitingForUser, _) => "Waiting for you".to_owned(),
        (ProductRunPhase::Cancelled, Some(_)) => "Cancelled — candidate available".to_owned(),
        (ProductRunPhase::Cancelled, None) => "Cancelled".to_owned(),
        (ProductRunPhase::RecoveryRequired, _) => "Recovery required".to_owned(),
        (ProductRunPhase::Failed, Some(_)) => "Candidate available".to_owned(),
        (ProductRunPhase::Failed, None) => "Stopped with no candidate".to_owned(),
        (phase, _) => phase_line(phase),
    }
}

fn product_state_style(run: &ProductRunSnapshot) -> Style {
    if run.phase() == ProductRunPhase::Failed && run.deliverable().is_some() {
        Style::default().fg(WARN)
    } else {
        phase_style(run.phase())
    }
}

const fn qualification_name(stage: CandidateStage) -> &'static str {
    match stage {
        CandidateStage::Observed => "observed",
        CandidateStage::Changed => "changed; checks missing",
        CandidateStage::SelfChecked => "self-checked; independent evidence missing",
        CandidateStage::GatesPassed => "deterministic checks passed; review missing",
        CandidateStage::ReviewPending => "review pending",
        CandidateStage::Qualified => "qualified",
    }
}

const fn evidence_name(evidence: &EvidenceStatus<QualificationEvidence>) -> &'static str {
    if let EvidenceStatus::Current(record) = evidence {
        return if record.value().satisfied() { "passed" } else { "failed" };
    }
    match evidence {
        EvidenceStatus::Missing => "missing",
        EvidenceStatus::Failed(_) => "failed",
        EvidenceStatus::Stale(_) => "stale",
        EvidenceStatus::Current(_) => unreachable!(),
    }
}

fn empty_detail() -> Text<'static> {
    Text::from(vec![
        Line::from("Ready."),
        Line::from("Press n and describe a useful coding outcome."),
    ])
}

fn conversation_text(conversation: Option<&ProductRunConversation>) -> Text<'static> {
    let Some(conversation) = conversation else {
        return Text::from(vec![
            Line::styled("Select a run to load its conversation.", Style::default().fg(MUTED)),
            Line::from("Press Enter or m to send a message."),
        ]);
    };
    let start = conversation.messages().len().saturating_sub(12);
    let mut lines = Vec::new();
    for message in &conversation.messages()[start..] {
        let (speaker, style) = match message.role() {
            ProductConversationRole::User => ("You", Style::default().fg(Color::White)),
            ProductConversationRole::Agent => ("Peritus", Style::default().fg(ACCENT)),
        };
        lines.push(Line::styled(speaker, style.add_modifier(Modifier::BOLD)));
        lines.extend(safe(message.content()).lines().map(|line| Line::from(line.to_owned())));
        lines.push(Line::from(""));
    }
    if lines.is_empty() {
        lines.push(Line::styled("No messages yet.", Style::default().fg(MUTED)));
    }
    Text::from(lines)
}

const fn phase_symbol(phase: ProductRunPhase) -> &'static str {
    match phase {
        ProductRunPhase::Queued => "○ Queued",
        ProductRunPhase::Designing => "● Designing",
        ProductRunPhase::Writing => "● Writing",
        ProductRunPhase::Checking => "● Checking",
        ProductRunPhase::Reviewing => "● Reviewing",
        ProductRunPhase::Fixing => "● Fixing",
        ProductRunPhase::Verifying => "● Verifying",
        ProductRunPhase::Complete => "✓ Complete",
        ProductRunPhase::Failed => "✗ Failed",
        ProductRunPhase::Cancelled => "■ Cancelled",
        ProductRunPhase::RecoveryRequired => "! Recover",
        ProductRunPhase::WaitingForUser => "? Your reply",
    }
}

fn phase_line(phase: ProductRunPhase) -> String {
    phase_symbol(phase).to_owned()
}

fn phase_style(phase: ProductRunPhase) -> Style {
    match phase {
        ProductRunPhase::Complete => Style::default().fg(GOOD),
        ProductRunPhase::Failed | ProductRunPhase::Cancelled => Style::default().fg(BAD),
        ProductRunPhase::RecoveryRequired | ProductRunPhase::WaitingForUser => {
            Style::default().fg(WARN)
        }
        _ => Style::default().fg(ACCENT),
    }
}

fn timeline(phase: ProductRunPhase) -> String {
    let active = match phase {
        ProductRunPhase::Queued | ProductRunPhase::Designing => 0,
        ProductRunPhase::Writing => 1,
        ProductRunPhase::Checking => 2,
        ProductRunPhase::Reviewing => 3,
        ProductRunPhase::Fixing => 4,
        ProductRunPhase::Verifying => 5,
        ProductRunPhase::Complete => 6,
        ProductRunPhase::Failed
        | ProductRunPhase::Cancelled
        | ProductRunPhase::RecoveryRequired
        | ProductRunPhase::WaitingForUser => 7,
    };
    ["Design", "Write", "Check", "Review", "Fix", "Verify", "Complete"]
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

#[cfg(test)]
mod tests;
