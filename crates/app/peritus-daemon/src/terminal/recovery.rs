//! Honest restart classification when no in-memory PTY control can survive daemon loss.

use peritus_process::{ProcessStore, RecoveryDisposition, RecoveryReport, TerminalResult};
use peritus_types::ProcessId;

/// Restart-visible terminal relation; none of these values grants a live attachment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RestartTerminalDisposition {
    /// C2 has a complete durable terminal result that may be presented as history.
    PersistedTerminal(TerminalResult),
    /// C2 found and reconciled an exact live tree, but its daemon-owned PTY control was lost.
    LiveControlUnavailable,
    /// C2 observed that the exact durable process identity was absent without a terminal fact.
    AbsentUnobserved,
    /// C2 could not establish an exact safe process identity or terminal fact.
    Indeterminate,
    /// The process was not present in the bounded reconciliation report.
    UnknownProcess,
}

impl RestartTerminalDisposition {
    /// Returns whether this restart observation permits A3 terminal attachment.
    #[must_use]
    pub(crate) const fn permits_attachment(&self) -> bool {
        false
    }
}

/// Classifies one process after C2 restart reconciliation without manufacturing a live control.
#[must_use]
pub(crate) fn classify_restart(
    store: &ProcessStore,
    report: &RecoveryReport,
    process_id: ProcessId,
) -> RestartTerminalDisposition {
    if let Ok(result) = store.terminal_result(process_id) {
        return RestartTerminalDisposition::PersistedTerminal(result);
    }
    report.entries().iter().find(|entry| entry.process_id() == process_id).map_or(
        RestartTerminalDisposition::UnknownProcess,
        |entry| match entry.disposition() {
            RecoveryDisposition::AlreadyTerminal => RestartTerminalDisposition::Indeterminate,
            RecoveryDisposition::LiveOwned => RestartTerminalDisposition::LiveControlUnavailable,
            RecoveryDisposition::AbsentUnobserved => RestartTerminalDisposition::AbsentUnobserved,
            RecoveryDisposition::Indeterminate => RestartTerminalDisposition::Indeterminate,
        },
    )
}
