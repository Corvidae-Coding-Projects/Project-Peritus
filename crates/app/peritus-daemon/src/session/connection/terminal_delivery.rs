//! Stable public diagnostics for attachment-local terminal delivery loss.

use peritus_app_protocol::{AppDiagnostic, AppEventPayload, AppProtocolLimits};

use crate::{DaemonError, terminal::TerminalBridgeError};

pub(super) fn terminal_gap_diagnostic(
    missing_bytes: u64,
    maximum_diagnostic_bytes: usize,
) -> Result<AppEventPayload, DaemonError> {
    let text = format!(
        "Terminal delivery omitted {missing_bytes} retained-history bytes; this attachment was released and the process remains owned. Reattach with terminal output-gap support."
    );
    constrained(text, maximum_diagnostic_bytes)
}

pub(super) fn terminal_diagnostic(
    error: &TerminalBridgeError,
    maximum_diagnostic_bytes: usize,
) -> Result<AppEventPayload, DaemonError> {
    // Only the stable category is public, never provider/process diagnostic contents.
    let text = format!(
        "Terminal delivery unavailable ({:?}); process state is unchanged. Inspect the preview or explicitly cancel it.",
        error.kind()
    );
    constrained(text, maximum_diagnostic_bytes)
}

fn constrained(
    text: String,
    maximum_diagnostic_bytes: usize,
) -> Result<AppEventPayload, DaemonError> {
    AppDiagnostic::new(text, AppProtocolLimits::PRODUCTION.max_diagnostic_bytes())
        .map_err(|_| super::invalid("terminal diagnostic exceeds its production bound"))?
        .constrained(maximum_diagnostic_bytes)
        .map(AppEventPayload::Diagnostic)
        .ok_or_else(|| super::invalid("terminal diagnostic limit cannot retain UTF-8 text"))
}
