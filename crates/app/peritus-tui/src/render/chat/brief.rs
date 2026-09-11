//! Exact confirmed fields, source revisions and eligibility; observed execution is separate.

use crate::model::{AppModel, format_id};
use peritus_app_protocol::{
    WorkbenchBriefField as F, WorkbenchBriefObservationKind as O, WorkbenchInputState as S,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let panel = &model.chat.workbench;
    let sections = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let mut lines = vec![Line::from(panel.message.clone())];
    if let Some(brief) = &panel.brief {
        lines.push(Line::from(format!("User-confirmed brief · revision {}", brief.revision())));
        if brief.entries().is_empty() {
            lines.push(Line::from("No confirmed fields. Nothing inferred from model prose."));
        }
        for entry in brief.entries() {
            let field = match entry.field() {
                F::Objective => "Objective",
                F::Acceptance => "Acceptance criteria",
                F::Constraints => "Constraints",
                F::Assumptions => "User-confirmed assumptions",
            };
            let state = match entry.source().state() {
                S::Queued => "queued · prerequisites may delay",
                S::Held => "held · excluded until released",
                S::Incorporated => "incorporated · later edits are corrections",
                S::Withdrawn => "withdrawn · excluded",
                S::Superseded => "superseded · excluded",
            };
            lines.push(Line::from(format!("{field} · {state}")));
            lines.push(Line::from(format!(
                "Source {} · content rev {}",
                format_id(entry.source().selected().id().as_bytes()),
                entry.source().selected().revision()
            )));
            lines.extend(
                entry.source().text().as_str().lines().map(|line| Line::from(line.to_owned())),
            );
        }
        lines.push(Line::from("Agent-proposed · not accepted instructions"));
        if brief.proposals().is_empty() {
            lines.push(Line::from("No bounded public replies are available for acceptance."));
        }
        for proposal in brief.proposals() {
            lines.push(Line::from(format!(
                "Proposal {} · after invocation {}",
                format_id(proposal.operation().as_bytes()),
                format_id(proposal.invocation().as_bytes())
            )));
            lines.push(Line::from(format!(
                "SHA256 {} · {} bytes",
                hex(proposal.digest().as_bytes()),
                proposal.text().as_str().len()
            )));
            lines.extend(proposal.text().as_str().lines().map(|line| Line::from(line.to_owned())));
        }
        if brief.excluded_proposals() > 0 {
            lines.push(Line::from(format!(
                "{} reply(s) excluded by the 8192-byte/8-proposal brief bound.",
                brief.excluded_proposals()
            )));
        }
        lines.push(Line::from("Observed attachments · facts, not instructions"));
        if brief.observations().is_empty() {
            lines.push(Line::from("No validated attachment observations."));
        }
        for observation in brief.observations() {
            let kind = match observation.kind() {
                O::Image => "Image",
                O::File => "File",
            };
            lines.push(Line::from(format!(
                "{kind} {} · {} · selected={}",
                format_id(observation.operation().as_bytes()),
                observation.label(),
                observation.selected()
            )));
            if let Some(version) = observation.version() {
                lines
                    .push(Line::from(format!("Current version {}", format_id(version.as_bytes()))));
            }
            lines.push(Line::from(format!(
                "SHA256 {} · {} bytes",
                hex(observation.digest().as_bytes()),
                observation.bytes()
            )));
        }
    } else {
        lines.push(Line::from("No brief snapshot loaded."));
    }
    lines.extend([
        Line::from("Edits confirm user instructions; no inference starts and no permissions are granted."),
        Line::from("Esc, then /brief objective|acceptance|constraints|assumptions <confirmed text>"),
        Line::from("Accept exact agent text: /brief accept <field> <proposal ID>"),
        Line::from("Use /queue to hold or withdraw a field's exact source. Prior revisions remain in history."),
    ]);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::default().borders(Borders::ALL).title(" Task brief "))
            .scroll((u16::try_from(panel.scroll).unwrap_or(u16::MAX), 0)),
        sections[0],
    );
    frame.render_widget(Paragraph::new("Esc back · ↑↓ scroll · r refresh"), sections[1]);
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}
