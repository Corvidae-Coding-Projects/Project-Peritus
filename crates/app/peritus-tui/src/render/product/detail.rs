//! Run status, deliverable, conversation, phase, and scrollable text rendering.

use peritus_app_protocol::{
    ProductActivityKind, ProductInteractionSnapshot, ProductRunOperationState, ProductRunPhase,
    ProductRunSnapshot,
};
use peritus_run_settlement::{
    CandidateStage, EvidenceStatus, QualificationEvidence, RunSettlement,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    model::{AppModel, format_digest},
    render::{ACCENT, BAD, GOOD, MUTED, WARN, field, short_text},
};

pub(super) fn render_run_text(
    frame: &mut Frame<'_>,
    area: Rect,
    model: &AppModel,
    title: &str,
    select: impl FnOnce(&ProductRunSnapshot) -> String,
    empty: &str,
) {
    let lines = run_text_lines(model, area.width, select, empty);
    let maximum = super::content_scroll_limit(lines.len(), area);
    let paragraph = Paragraph::new(lines).block(
        Block::default().borders(Borders::ALL).title(format!("{title}· PgUp/PgDn · Home/End ")),
    );
    let scroll = model.product.as_ref().map_or(0, |product| product.inspection_scroll).min(maximum);
    frame.render_widget(paragraph.scroll((scroll, 0)), area);
}

pub(super) fn run_text_lines(
    model: &AppModel,
    width: u16,
    select: impl FnOnce(&ProductRunSnapshot) -> String,
    empty: &str,
) -> Vec<Line<'static>> {
    let text = model.product.as_ref().and_then(|product| product.selected_run()).map_or_else(
        || empty.to_owned(),
        |run| {
            let value = select(run);
            if value.is_empty() { empty.to_owned() } else { safe(&value) }
        },
    );
    crate::render::chat::wrapped_lines(
        text.lines().map(|line| Line::from(line.to_owned())).collect(),
        usize::from(width.saturating_sub(2)),
    )
}

pub(super) fn run_detail(
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
        Line::from(""),
        Line::styled(
            "Operation authority",
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        field("Knowledge", format!("{:?}", run.operation().state())),
        field(
            "Uncertain",
            if run.operation().uncertainty().is_empty() {
                "nothing material".to_owned()
            } else {
                safe(run.operation().uncertainty())
            },
        ),
        field("Known", safe(run.operation().known())),
        field("Legal controls", legal_controls(run)),
        field("Identity", safe(run.operation().identity())),
        field("Kind", format!("{:?}", run.operation().kind())),
        Line::from(""),
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
            let identity = checkpoint.identity();
            lines.push(field(
                "Content",
                short_text(&format_digest(identity.content_digest().as_bytes()), 16),
            ));
            lines.push(field(
                "Repository context",
                short_text(&format_digest(identity.repository_digest().as_bytes()), 16),
            ));
            lines
                .push(field("Requirements revision", identity.requirements_revision().to_string()));
            lines.push(field(
                "Execution context",
                identity.execution_digest().map_or_else(
                    || "not observed".to_owned(),
                    |digest| short_text(&format_digest(digest.as_bytes()), 16),
                ),
            ));
            lines.push(field("Checks", evidence_name(checkpoint.gates())));
            lines.push(field("Requirements", evidence_name(checkpoint.obligations())));
            lines.push(field("Review", evidence_name(checkpoint.review())));
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

pub(super) fn inspect_text(run: &ProductRunSnapshot) -> String {
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
        "Operation\n{}\nKnown\n{}\nUncertain\n{}\nLegal controls\n{}\n\nWorkspace\n{}\n\nStatus\n{}\n\nExact candidate paths\n{}\n\nSuccessful commands\n{}\n\nRun instructions\n{}\n\nDiff\n{}",
        run.operation().identity(),
        run.operation().known(),
        if run.operation().uncertainty().is_empty() {
            "nothing material"
        } else {
            run.operation().uncertainty()
        },
        legal_controls(run),
        deliverable.workspace_path(),
        run.status(),
        paths,
        commands,
        deliverable.run_instructions(),
        run.diff(),
    )
}

pub(super) fn product_state(run: &ProductRunSnapshot) -> String {
    if run.operation().state() == ProductRunOperationState::OutcomeUnknown {
        return "Outcome unknown — inspect before recovery".to_owned();
    }
    match (run.phase(), run.deliverable()) {
        (_, Some(deliverable)) if deliverable.discarded() => "Discarded".to_owned(),
        (_, Some(deliverable)) if !deliverable.commit_revision().is_empty() => {
            "Committed".to_owned()
        }
        (_, Some(deliverable)) if deliverable.accepted() => "Accepted".to_owned(),
        (ProductRunPhase::Complete, Some(_)) => "Ready for inspection".to_owned(),
        (ProductRunPhase::Complete, None) => "Complete".to_owned(),
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
    if run.operation().state() == ProductRunOperationState::OutcomeUnknown {
        Style::default().fg(WARN)
    } else if run.deliverable().is_some_and(peritus_app_protocol::ProductDeliverable::discarded) {
        Style::default().fg(MUTED)
    } else if run.phase() == ProductRunPhase::Failed && run.deliverable().is_some() {
        Style::default().fg(WARN)
    } else {
        phase_style(run.phase())
    }
}

fn legal_controls(run: &ProductRunSnapshot) -> String {
    let controls = run.operation().legal_controls();
    let mut values = Vec::new();
    for (allowed, name) in [
        (controls.cancel(), "cancel"),
        (controls.retry(), "exact retry"),
        (controls.accept(), "accept"),
        (controls.commit(), "commit"),
        (controls.export(), "export"),
        (controls.discard(), "discard"),
        (controls.acknowledge(), "acknowledge uncertainty"),
    ] {
        if allowed {
            values.push(name);
        }
    }
    if values.is_empty() { "none".to_owned() } else { values.join(", ") }
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

fn evidence_name(evidence: &EvidenceStatus<QualificationEvidence>) -> String {
    let state = match evidence {
        EvidenceStatus::Missing => return "missing".to_owned(),
        EvidenceStatus::Stale(_) => "stale",
        EvidenceStatus::Current(record) if record.value().satisfied() => "passed",
        EvidenceStatus::Failed(_) | EvidenceStatus::Current(_) => "failed",
    };
    let dependencies = evidence.record().map_or_else(String::new, |record| {
        let dependency = record.dependencies();
        if dependency.execution() {
            "content + requirements + execution".to_owned()
        } else {
            "content + requirements".to_owned()
        }
    });
    format!("{state} · {dependencies}")
}

pub(super) fn empty_detail() -> Text<'static> {
    Text::from(vec![
        Line::from("Ready."),
        Line::from("Press Esc, then use /build <request> in Conversation."),
    ])
}

pub(super) fn conversation_text(
    conversation: Option<&ProductInteractionSnapshot>,
    activities: Option<&[peritus_app_protocol::ProductActivity]>,
    complete: bool,
    unavailable: u64,
) -> Text<'static> {
    let Some(conversation) = conversation else {
        return Text::from(vec![
            Line::styled("Select a run to load its conversation.", Style::default().fg(MUTED)),
            Line::from("Press Enter or m to send a message."),
        ]);
    };
    let mut lines = Vec::new();
    let activities = activities.unwrap_or_else(|| conversation.activities());
    if unavailable != 0 {
        lines.push(Line::styled(
            format!("{unavailable} earliest activities predate complete history retention"),
            Style::default().fg(MUTED),
        ));
        lines.push(Line::from(""));
    }
    if let Some(window) = conversation.activity_window().filter(|_| !complete) {
        lines.push(Line::styled(
            format!(
                "Loading complete history · {} earlier activities omitted from this live view",
                window.omitted()
            ),
            Style::default().fg(MUTED),
        ));
        if window.omitted_errors() != 0 {
            lines.push(Line::styled(
                format!("{} earlier errors are outside this live view", window.omitted_errors()),
                Style::default().fg(Color::Red),
            ));
        }
        lines.push(Line::from(""));
    }
    let start = activities.len().saturating_sub(12);
    for activity in &activities[start..] {
        let (speaker, style) = match activity.kind() {
            ProductActivityKind::User => ("You", Style::default().fg(Color::White)),
            ProductActivityKind::Assistant => ("Peritus", Style::default().fg(ACCENT)),
            ProductActivityKind::Tool => ("Tool", Style::default().fg(Color::Cyan)),
            ProductActivityKind::Status => ("Status", Style::default().fg(MUTED)),
            ProductActivityKind::Error => ("Error", Style::default().fg(Color::Red)),
        };
        lines.push(Line::styled(speaker, style.add_modifier(Modifier::BOLD)));
        lines.extend(safe(activity.text()).lines().map(|line| Line::from(line.to_owned())));
        if !activity.detail().is_empty() {
            lines.extend(
                safe(activity.detail())
                    .lines()
                    .map(|line| Line::styled(line.to_owned(), Style::default().fg(MUTED))),
            );
        }
        lines.push(Line::from(""));
    }
    if let Some(activity) = conversation
        .activity_window()
        .and_then(peritus_app_protocol::ProductActivityWindow::terminal_error)
    {
        lines.push(Line::styled(
            "Error",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
        lines.extend(safe(activity.text()).lines().map(|line| Line::from(line.to_owned())));
        lines.extend(
            safe(activity.detail())
                .lines()
                .map(|line| Line::styled(line.to_owned(), Style::default().fg(MUTED))),
        );
        lines.push(Line::from(""));
    }
    if lines.is_empty() {
        lines.push(Line::styled("No messages yet.", Style::default().fg(MUTED)));
    }
    Text::from(lines)
}

pub(super) const fn phase_symbol(phase: ProductRunPhase) -> &'static str {
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

pub(super) fn safe(value: &str) -> String {
    value
        .chars()
        .filter(|character| *character == '\n' || *character == '\t' || !character.is_control())
        .collect()
}
