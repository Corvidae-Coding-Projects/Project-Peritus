//! Exact covered-path checkpoint and rewind preview without workspace mutation on inspection.

use crate::model::{AppModel, format_id};
use peritus_app_protocol::{
    WorkbenchCheckpointFileMode, WorkbenchCheckpointVersion, WorkbenchRestoreStatus,
    WorkbenchRewindDisposition,
};
use ratatui::{Frame, layout::Rect};

pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, model: &AppModel) {
    super::inspector::draw(
        frame,
        area,
        model,
        content(model),
        " Checkpoint · safe rewind ",
        "Esc back · ↑↓/PgUp/PgDn scroll · Home/End · c confirm · r refresh",
    );
}

pub(super) fn content(model: &AppModel) -> Vec<String> {
    let panel = &model.chat.workbench;
    let mut lines = vec![panel.message.clone()];
    if let Some(preview) = &panel.rewind_preview {
        lines.push(format!(
            "Checkpoint {} · inspected revision {}",
            format_id(preview.request().checkpoint().as_bytes()),
            preview.request().revision()
        ));
        append_scope(&mut lines, preview.request());
        lines.push(format!("Exact preview SHA256 {}", hex(preview.preview_digest().as_bytes())));
        for path in preview.paths() {
            lines.push(format!("[{}] {}", disposition(path.disposition()), path.path()));
            append_coverage(&mut lines, path.coverage());
            lines.push(format!(
                "  checkpoint {} · expected current {} · observed {}",
                version(path.checkpoint()),
                path.expected_current().map_or_else(|| "unsealed".to_owned(), version),
                version(path.observed_current())
            ));
        }
        append_named(&mut lines, "Excluded", preview.exclusions());
        append_named(&mut lines, "Not restored", preview.external_effects());
        lines.push(format!(
            "Conversation history preserved: {} · cumulative accounting preserved: {}",
            preview.conversation_history_preserved(),
            preview.accounting_preserved()
        ));
        lines.push(String::from(
            "Press c to confirm this exact preview. Conflicts and unsealed paths are never overwritten; Esc cancels without a request.",
        ));
    } else if let Some(receipt) = &panel.restore_receipt {
        lines.extend([
            format!(
                "Restore {} · status {} · durable revision {}",
                format_id(receipt.restore().as_bytes()),
                restore_status(receipt.status()),
                receipt.accepted_revision()
            ),
            format!(
                "Source checkpoint {} · recovery checkpoint {}",
                format_id(receipt.checkpoint().as_bytes()),
                format_id(receipt.recovery_checkpoint().as_bytes())
            ),
        ]);
        append_named(&mut lines, "Restored", receipt.restored());
        append_named(&mut lines, "Conflicts retained", receipt.conflicts());
        append_named(&mut lines, "Not restored", receipt.external_effects());
        lines.push(String::from(
            "Original conversation history and cumulative accounting remain preserved.",
        ));
    } else if let Some(receipt) = &panel.checkpoint_receipt {
        append_checkpoint(&mut lines, receipt);
    } else {
        lines.push(String::from(
            "No checkpoint receipt or rewind preview loaded. Use /checkpoint [name], /checkpoint show <checkpoint-id>, or /rewind <checkpoint-id> [files|conversation|combined].",
        ));
    }
    lines
}

fn append_checkpoint(
    lines: &mut Vec<String>,
    receipt: &peritus_app_protocol::WorkbenchCheckpointReceipt,
) {
    let references = receipt.references();
    lines.extend([
        format!(
            "Checkpoint {} · {} · durable revision {}",
            format_id(receipt.checkpoint().as_bytes()),
            receipt.name().as_str(),
            receipt.accepted_revision()
        ),
        format!(
            "References: conversation {} · context {} · brief {} · goal {}",
            references.source_conversation_revision(),
            references.context_generation(),
            references.brief_revision(),
            references.goal_revision().map_or_else(|| "none".to_owned(), |value| value.to_string())
        ),
    ]);
    if receipt.paths().is_empty() {
        lines.push(String::from(
            "No workspace targets are covered. Attach a file or selected range with /files <path>, then create another checkpoint before editing.",
        ));
    }
    for path in receipt.paths() {
        append_coverage(lines, path.coverage());
        lines.push(format!(
            "Covered {} · checkpoint {} · owned current {}",
            path.path(),
            version(path.checkpoint()),
            path.expected_current().map_or_else(|| "awaiting completed run".to_owned(), version)
        ));
    }
    append_named(lines, "Excluded", receipt.exclusions());
    append_named(lines, "Not restored", receipt.external_effects());
    lines.push(format!(
        "After a completed owned edit, use /rewind {} [files|conversation|combined] to inspect the exact scope.",
        format_id(receipt.checkpoint().as_bytes())
    ));
}

fn append_scope(lines: &mut Vec<String>, request: peritus_app_protocol::WorkbenchRewindRequest) {
    lines.push(format!("Scope: {:?}", request.mode()));
    if let Some(child) = request.child() {
        lines.push(format!(
            "New read-only logical branch: {} (no historical file claim unless combined settles)",
            format_id(child.as_bytes())
        ));
    }
}

fn append_named(lines: &mut Vec<String>, label: &str, values: &[String]) {
    for value in values {
        lines.push(format!("{label}: {value}"));
    }
}

const fn disposition(value: WorkbenchRewindDisposition) -> &'static str {
    match value {
        WorkbenchRewindDisposition::Restore => "RESTORE",
        WorkbenchRewindDisposition::Unchanged => "UNCHANGED",
        WorkbenchRewindDisposition::Conflict => "CONFLICT — KEEP CURRENT",
        WorkbenchRewindDisposition::Unsealed => "UNSEALED — KEEP CURRENT",
        WorkbenchRewindDisposition::Unavailable => "SELECTED RANGE UNAVAILABLE — KEEP CURRENT",
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
        WorkbenchCheckpointVersion::EmptyDirectory { permissions } => {
            format!("empty directory · permissions {permissions:04o}")
        }
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

fn append_coverage(
    lines: &mut Vec<String>,
    coverage: peritus_app_protocol::WorkbenchCheckpointCoverage<'_>,
) {
    use peritus_app_protocol::{WorkbenchCheckpointCoverage, WorkbenchFileRange};
    match coverage {
        WorkbenchCheckpointCoverage::WholeFile => {
            lines.push("  Scope: complete file and mode".to_owned());
        }
        WorkbenchCheckpointCoverage::AbsentPath => lines.push("  Scope: absent target".to_owned()),
        WorkbenchCheckpointCoverage::EmptyDirectory => {
            lines.push("  Scope: empty directory and permissions".to_owned());
        }
        WorkbenchCheckpointCoverage::SelectedRanges(ranges) => {
            lines.push(
                "  Scope: selected ranges; surrounding current bytes and mode are preserved"
                    .to_owned(),
            );
            for range in ranges {
                let (start, end) = range.captured_interval();
                let selection = match range.selection() {
                    WorkbenchFileRange::All => "complete file".to_owned(),
                    WorkbenchFileRange::Bytes { start, end } => format!("bytes {start}..{end}"),
                    WorkbenchFileRange::Lines { first, last } => format!("lines {first}..={last}"),
                };
                lines.push(format!("    {selection} · captured bytes {start}..{end}"));
            }
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}
