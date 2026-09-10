//! Exact covered-path checkpoint and rewind preview without workspace mutation on inspection.

use crate::model::{AppModel, format_id};
use peritus_app_protocol::{
    WorkbenchCheckpointFileMode, WorkbenchCheckpointVersion, WorkbenchRestoreStatus,
    WorkbenchRewindDisposition,
};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    let panel = &model.chat.workbench;
    let areas = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).split(area);
    let mut lines = vec![Line::from(panel.message.clone())];
    if let Some(preview) = &panel.rewind_preview {
        lines.push(Line::from(format!(
            "Checkpoint {} · inspected revision {}",
            format_id(preview.request().checkpoint().as_bytes()),
            preview.request().revision()
        )));
        append_scope(&mut lines, preview.request());
        lines.push(Line::from(format!(
            "Exact preview SHA256 {}",
            hex(preview.preview_digest().as_bytes())
        )));
        for path in preview.paths() {
            lines.push(Line::from(format!(
                "[{}] {}",
                disposition(path.disposition()),
                path.path()
            )));
            lines.push(Line::from(format!(
                "  checkpoint {} · expected current {} · observed {}",
                version(path.checkpoint()),
                path.expected_current().map_or_else(|| "unsealed".to_owned(), version),
                version(path.observed_current())
            )));
        }
        append_named(&mut lines, "Excluded", preview.exclusions());
        append_named(&mut lines, "Not restored", preview.external_effects());
        lines.push(Line::from(format!(
            "Conversation history preserved: {} · cumulative accounting preserved: {}",
            preview.conversation_history_preserved(),
            preview.accounting_preserved()
        )));
        lines.push(Line::from(
            "Press c to confirm this exact preview. Conflicts and unsealed paths are never overwritten; Esc cancels without a request.",
        ));
    } else if let Some(receipt) = &panel.restore_receipt {
        lines.extend([
            Line::from(format!(
                "Restore {} · status {} · durable revision {}",
                format_id(receipt.restore().as_bytes()),
                restore_status(receipt.status()),
                receipt.accepted_revision()
            )),
            Line::from(format!(
                "Source checkpoint {} · recovery checkpoint {}",
                format_id(receipt.checkpoint().as_bytes()),
                format_id(receipt.recovery_checkpoint().as_bytes())
            )),
        ]);
        append_named(&mut lines, "Restored", receipt.restored());
        append_named(&mut lines, "Conflicts retained", receipt.conflicts());
        append_named(&mut lines, "Not restored", receipt.external_effects());
        lines.push(Line::from(
            "Original conversation history and cumulative accounting remain preserved.",
        ));
    } else if let Some(receipt) = &panel.checkpoint_receipt {
        let references = receipt.references();
        lines.extend([
            Line::from(format!(
                "Checkpoint {} · {} · durable revision {}",
                format_id(receipt.checkpoint().as_bytes()),
                receipt.name().as_str(),
                receipt.accepted_revision()
            )),
            Line::from(format!(
                "References: conversation {} · context {} · brief {} · goal {}",
                references.source_conversation_revision(),
                references.context_generation(),
                references.brief_revision(),
                references
                    .goal_revision()
                    .map_or_else(|| "none".to_owned(), |value| value.to_string())
            )),
        ]);
        for path in receipt.paths() {
            lines.push(Line::from(format!(
                "Covered {} · checkpoint {} · owned current {}",
                path.path(),
                version(path.checkpoint()),
                path.expected_current()
                    .map_or_else(|| "awaiting completed run".to_owned(), version)
            )));
        }
        append_named(&mut lines, "Excluded", receipt.exclusions());
        append_named(&mut lines, "Not restored", receipt.external_effects());
        lines.push(Line::from(format!(
            "After a completed owned edit, use /rewind {} [files|conversation|combined] to inspect the exact scope.",
            format_id(receipt.checkpoint().as_bytes())
        )));
    } else {
        lines.push(Line::from(
            "No checkpoint receipt or rewind preview loaded. Use /checkpoint [name], /checkpoint show <checkpoint-id>, or /rewind <checkpoint-id> [files|conversation|combined].",
        ));
    }
    finish_draw(frame, areas[0], areas[1], panel.scroll, lines);
}

fn append_scope(
    lines: &mut Vec<Line<'static>>,
    request: peritus_app_protocol::WorkbenchRewindRequest,
) {
    lines.push(Line::from(format!("Scope: {:?}", request.mode())));
    if let Some(child) = request.child() {
        lines.push(Line::from(format!(
            "New read-only logical branch: {} (no historical file claim unless combined settles)",
            format_id(child.as_bytes())
        )));
    }
    if let Some(budget) = request.allocation() {
        lines.push(Line::from(format!(
            "Reserved child budget: {} ms · {} requests · {} tools · {} tokens",
            budget.active_millis(),
            budget.requests(),
            budget.tool_calls(),
            budget.total_tokens()
        )));
    }
}

fn finish_draw(
    frame: &mut Frame<'_>,
    content_area: Rect,
    footer_area: Rect,
    scroll: usize,
    lines: Vec<Line<'static>>,
) {
    let content = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title(" Checkpoint · safe rewind "))
        .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0));
    frame.render_widget(content, content_area);
    draw_footer(frame, footer_area);
}

fn draw_footer(frame: &mut Frame<'_>, area: Rect) {
    frame.render_widget(
        Paragraph::new("Esc cancel/back · ↑↓ scroll · c confirm exact preview · r refresh"),
        area,
    );
}

fn append_named(lines: &mut Vec<Line<'static>>, label: &str, values: &[String]) {
    for value in values {
        lines.push(Line::from(format!("{label}: {value}")));
    }
}

const fn disposition(value: WorkbenchRewindDisposition) -> &'static str {
    match value {
        WorkbenchRewindDisposition::Restore => "RESTORE",
        WorkbenchRewindDisposition::Unchanged => "UNCHANGED",
        WorkbenchRewindDisposition::Conflict => "CONFLICT — KEEP CURRENT",
        WorkbenchRewindDisposition::Unsealed => "UNSEALED — KEEP CURRENT",
    }
}

const fn restore_status(value: WorkbenchRestoreStatus) -> &'static str {
    match value {
        WorkbenchRestoreStatus::Applied => "APPLIED",
        WorkbenchRestoreStatus::Conflict => "CONFLICT — NO CHANGES",
        WorkbenchRestoreStatus::RecoveryRequired => "RECOVERY REQUIRED",
    }
}

fn version(value: WorkbenchCheckpointVersion) -> String {
    match value {
        WorkbenchCheckpointVersion::Absent => "absent".to_owned(),
        WorkbenchCheckpointVersion::Present { digest, bytes, mode } => format!(
            "{} bytes · {} · SHA256 {}",
            bytes,
            match mode {
                WorkbenchCheckpointFileMode::Regular => "regular",
                WorkbenchCheckpointFileMode::Executable => "executable",
            },
            hex(digest.as_bytes())
        ),
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}
