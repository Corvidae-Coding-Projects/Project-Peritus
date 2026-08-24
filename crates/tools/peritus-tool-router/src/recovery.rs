//! Replay and restart recovery classifications.

/// Exact prior-call disposition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayDisposition {
    /// The exact call is already active.
    Active,
    /// A non-idempotent terminal outcome exists and is not automatically replayable.
    NonIdempotentTerminal,
    /// Effect outcome is indeterminate and must be reconciled.
    Indeterminate,
}

/// Result of explicit active-execution recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryClassification {
    /// Execution remains owned and active.
    Active,
    /// A terminal result was accepted and cached.
    Completed,
    /// Outcome cannot safely support success or automatic retry.
    Indeterminate,
}

/// Complete router recovery outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecoveryOutcome {
    /// Execution remains owned with a bounded update.
    Active(crate::ExecutionUpdate),
    /// Exact terminal envelope was accepted and cached.
    Completed(peritus_tool_protocol::ToolResult),
    /// Outcome is unsafe to infer or retry.
    Indeterminate(crate::DispatchFailure),
}
