//! Terminal user-facing summaries and explicit handoff failure projection.

use super::{ProductRunOutcome, ProductRunPhase, replace_snapshot};

pub(super) fn terminal_summary(outcome: &ProductRunOutcome, detail: &str) -> String {
    let mut summary = outcome
        .candidate()
        .map_or_else(|| detail.to_owned(), |candidate| candidate.summary.clone());
    if !detail.is_empty() && !summary.contains(detail) {
        summary.push_str("\n\nInterruption: ");
        summary.push_str(detail);
    }
    if !outcome.remaining_work().is_empty() && !summary.contains("Remaining work:") {
        summary.push_str("\n\nRemaining work:\n- ");
        summary.push_str(&outcome.remaining_work().join("\n- "));
    }
    summary
}

pub(super) fn fail_handoff(record: &mut super::super::RunRecord, reason: &str) {
    let detail = if record.interruption_cause.is_empty() {
        reason.to_owned()
    } else {
        format!("{reason}: {}", record.interruption_cause)
    };
    if let Ok(snapshot) = replace_snapshot(
        &record.snapshot,
        ProductRunPhase::Failed,
        "Create durable deliverable handoff failed",
        &detail,
    ) {
        record.snapshot = snapshot;
    }
    let _ = record.interaction.append(
        peritus_app_protocol::ProductActivityKind::Status,
        &detail,
        "Durable handoff failure",
    );
}
