//! Work recovery and lifecycle phases.

use vstd::prelude::*;

verus! {

/// Worker-loss classification fixed at admission.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryPolicy {
    /// Ownership loss safely requeues under the next attempt.
    RetrySafe,
    /// Ownership loss has ambiguous external outcome and cannot be retried automatically.
    Ambiguous,
    /// Ownership loss is a terminal failure.
    Fail,
}

/// Closed work lifecycle.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkPhase {
    /// Waiting for successful dependencies.
    WaitingDependencies,
    /// Eligible for deterministic selection.
    Queued,
    /// Reserved durably but not yet acknowledged by the worker.
    Reserved,
    /// Worker acknowledged execution ownership.
    Running,
    /// Retryable failure awaits explicit retry command.
    RetryPending,
    /// Cancellation was requested and awaits owner acknowledgement.
    Cancelling,
    /// Immutable non-running outcome is retained.
    Terminal,
}

impl WorkPhase {
    /// Compares exact lifecycle variants for verified production admission.
    pub(crate) const fn same(self, other: Self) -> (result: bool)
        ensures result == (self == other),
    {
        matches!(
            (self, other),
            (Self::WaitingDependencies, Self::WaitingDependencies)
                | (Self::Queued, Self::Queued)
                | (Self::Reserved, Self::Reserved)
                | (Self::Running, Self::Running)
                | (Self::RetryPending, Self::RetryPending)
                | (Self::Cancelling, Self::Cancelling)
                | (Self::Terminal, Self::Terminal)
        )
    }
}

} // verus!
