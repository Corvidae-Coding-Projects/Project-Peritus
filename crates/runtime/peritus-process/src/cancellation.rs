//! Closed cancellation and escalation vocabulary.

/// First accepted reason for stopping an owned process tree.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CancellationReason {
    /// Explicit caller cancellation.
    User,
    /// Configured wall deadline elapsed.
    Deadline,
    /// An output ceiling was crossed.
    OutputLimit,
    /// A resource ceiling was crossed.
    ResourceLimit,
    /// The governing lease generation was fenced.
    LeaseFence,
    /// The owning supervisor is shutting down.
    SupervisorShutdown,
    /// The sandbox backend reported a terminal failure.
    BackendFailure,
}

/// Immutable first-trigger record used for deterministic terminal classification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StopTrigger {
    sequence: u64,
    reason: CancellationReason,
}

impl StopTrigger {
    /// Creates a nonzero trigger sequence.
    #[must_use]
    pub(crate) const fn new(sequence: u64, reason: CancellationReason) -> Self {
        Self { sequence, reason }
    }

    /// Returns the first accepted event sequence.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }
    /// Returns the stop reason.
    #[must_use]
    pub const fn reason(self) -> CancellationReason {
        self.reason
    }
}

/// Observed graceful and forced process-tree control.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EscalationRecord {
    graceful_attempted: bool,
    forced: bool,
    tree_quiescent: bool,
}

impl EscalationRecord {
    /// Creates a terminal escalation observation.
    #[must_use]
    pub const fn new(graceful_attempted: bool, forced: bool, tree_quiescent: bool) -> Self {
        Self { graceful_attempted, forced, tree_quiescent }
    }

    /// Returns whether a graceful action was attempted.
    #[must_use]
    pub const fn graceful_attempted(self) -> bool {
        self.graceful_attempted
    }
    /// Returns whether forced termination was required.
    #[must_use]
    pub const fn forced(self) -> bool {
        self.forced
    }
    /// Returns whether the complete owned tree became quiescent.
    #[must_use]
    pub const fn tree_quiescent(self) -> bool {
        self.tree_quiescent
    }
}
