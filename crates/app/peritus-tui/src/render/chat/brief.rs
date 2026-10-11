//! Exact confirmed fields, source revisions and eligibility; observed execution is separate.

use crate::model::{AppModel, format_id};
use peritus_app_protocol::{
    WorkbenchBriefField as F, WorkbenchBriefObservationKind as O, WorkbenchInputState as S,
};
use ratatui::{Frame, layout::Rect};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    super::inspector::draw(
        frame,
        area,
        model,
        content(model),
        " Task brief ",
        "Esc back · ↑↓/PgUp/PgDn scroll · Home/End · r refresh",
    );
}

pub(super) fn content(model: &AppModel) -> Vec<String> {
    let panel = &model.chat.workbench;
    let mut lines = vec![panel.message.clone()];
    if let Some(brief) = &panel.brief {
        lines.push(format!("User-confirmed brief · revision {}", brief.revision()));
        if brief.entries().is_empty() {
            lines.push(String::from("No confirmed fields. Nothing inferred from model prose."));
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
            lines.push(format!("{field} · {state}"));
            lines.push(format!(
                "Source {} · content rev {}",
                format_id(entry.source().selected().id().as_bytes()),
                entry.source().selected().revision()
            ));
            lines.extend(entry.source().text().as_str().lines().map(str::to_owned));
        }
        if let Some(page) = &panel.brief_page {
            paged_sources(&mut lines, page, panel.brief_body.as_ref());
        } else {
            legacy_sources(&mut lines, brief);
        }
    } else {
        lines.push(String::from("No brief snapshot loaded."));
    }
    lines.extend([
        String::from("Edits confirm user instructions; no inference starts and no permissions are granted."),
        String::from("Esc, then /brief objective|acceptance|constraints|assumptions <confirmed text>"),
        String::from("Accept exact agent text: /brief accept <field> <proposal ID>"),
        String::from("Use /queue to hold or withdraw a field's exact source. Prior revisions remain in history."),
    ]);
    lines
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

fn legacy_sources(lines: &mut Vec<String>, brief: &peritus_app_protocol::WorkbenchBrief) {
    lines.push(String::from("Agent-proposed · not accepted instructions"));
    if brief.proposals().is_empty() {
        lines.push(String::from("No bounded public replies are available for acceptance."));
    }
    for proposal in brief.proposals() {
        lines.push(format!(
            "Proposal {} · after invocation {}",
            format_id(proposal.operation().as_bytes()),
            format_id(proposal.invocation().as_bytes())
        ));
        lines.push(format!(
            "SHA256 {} · {} bytes",
            hex(proposal.digest().as_bytes()),
            proposal.text().len()
        ));
        lines.extend(proposal.text().lines().map(str::to_owned));
    }
    if brief.excluded_proposals() > 0 {
        lines.push(format!(
            "{} reply(s) cannot be represented by the legacy brief protocol.",
            brief.excluded_proposals()
        ));
    }
    lines.push(String::from("Observed attachments · facts, not instructions"));
    if brief.observations().is_empty() {
        lines.push(String::from("No validated attachment observations."));
    }
    for observation in brief.observations() {
        let kind = match observation.kind() {
            O::Image => "Image",
            O::File => "File",
        };
        lines.push(format!(
            "{kind} {} · {} · selected={}",
            format_id(observation.operation().as_bytes()),
            observation.label(),
            observation.selected()
        ));
        if let Some(version) = observation.version() {
            lines.push(format!("Current version {}", format_id(version.as_bytes())));
        }
        lines.push(format!(
            "SHA256 {} · {} bytes",
            hex(observation.digest().as_bytes()),
            observation.bytes()
        ));
    }
}

fn paged_sources(
    lines: &mut Vec<String>,
    page: &peritus_app_protocol::WorkbenchBriefPage,
    body: Option<&peritus_app_protocol::WorkbenchBriefProposalPage>,
) {
    lines.push(format!(
        "Agent-proposed · {} total · rows {}–{} · not accepted instructions",
        page.proposal_total(),
        page.request().proposals(),
        page.request().proposals() + page.proposals().len() as u64
    ));
    for proposal in page.proposals() {
        lines.push(format!(
            "Proposal {} · {} bytes · SHA256 {}",
            format_id(proposal.operation().as_bytes()),
            proposal.bytes(),
            hex(proposal.digest().as_bytes())
        ));
    }
    lines.push(format!(
        "Observed attachments · {} total · rows {}–{} · facts, not instructions",
        page.observation_total(),
        page.request().observations(),
        page.request().observations() + page.observations().len() as u64
    ));
    for observation in page.observations() {
        lines.push(format!(
            "{:?} {} · {} · selected={} · {} bytes · SHA256 {}",
            observation.kind(),
            format_id(observation.operation().as_bytes()),
            observation.label(),
            observation.selected(),
            observation.bytes(),
            hex(observation.digest().as_bytes())
        ));
        if let Some(version) = observation.version() {
            lines.push(format!("Current version {}", format_id(version.as_bytes())));
        }
    }
    lines.push(String::from("/brief next | previous · /brief show <proposal ID>"));
    if let Some(body) = body {
        let request = body.request();
        lines.push(format!(
            "Proposal {} · bytes {}–{} of {} · SHA256 {}",
            format_id(request.proposal().operation().as_bytes()),
            request.offset(),
            request.offset() + body.text().len() as u64,
            request.proposal().bytes(),
            hex(request.proposal().digest().as_bytes())
        ));
        lines.push(String::from("Source controls are escaped for display. Acceptance retains the complete original bytes."));
        let escaped: String = body
            .text()
            .chars()
            .flat_map(|character| {
                if character.is_control() && character != '\n' {
                    character.escape_default().collect::<Vec<_>>()
                } else {
                    vec![character]
                }
            })
            .collect();
        lines.extend(escaped.lines().map(str::to_owned));
        lines.push(String::from("/brief text next | previous · acceptance selects the complete proposal, including pages not displayed here."));
    }
}
