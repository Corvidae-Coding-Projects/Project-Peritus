//! Run status, deliverable, conversation, phase, and scrollable text rendering.

use peritus_app_protocol::{
    ProductConversationRole, ProductRunConversation, ProductRunPhase, ProductRunSnapshot,
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
    model::AppModel,
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
    let text = model.product.as_ref().and_then(|product| product.selected_run()).map_or_else(
        || empty.to_owned(),
        |run| {
            let value = select(run);
            if value.is_empty() { empty.to_owned() } else { safe(&value) }
        },
    );
    let lines = crate::render::chat::wrapped_lines(
        text.lines().map(|line| Line::from(line.to_owned())).collect(),
        usize::from(area.width.saturating_sub(2)),
    );
    let maximum = lines.len().saturating_sub(usize::from(area.height.saturating_sub(2)));
    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(format!("{title}· PgUp/PgDn · Home ")));
    let scroll = usize::from(model.product.as_ref().map_or(0, |product| product.inspection_scroll))
        .min(maximum);
    frame.render_widget(paragraph.scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)), area);
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
        "Workspace\n{}\n\nExact candidate paths\n{}\n\nSuccessful commands\n{}\n\nRun instructions\n{}\n\nDiff\n{}",
        deliverable.workspace_path(),
        paths,
        commands,
        deliverable.run_instructions(),
        run.diff(),
    )
}

pub(super) fn product_state(run: &ProductRunSnapshot) -> String {
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

pub(super) fn empty_detail() -> Text<'static> {
    Text::from(vec![
        Line::from("Ready."),
        Line::from("Press n and describe a useful coding outcome."),
    ])
}

pub(super) fn conversation_text(conversation: Option<&ProductRunConversation>) -> Text<'static> {
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
