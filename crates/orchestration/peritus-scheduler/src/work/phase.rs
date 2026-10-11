//! Work recovery and lifecycle phases.

#[doc = " Worker-loss classification fixed at admission."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RecoveryPolicy {
    #[doc = " Ownership loss safely requeues under the next attempt."]
    RetrySafe,
    #[doc = " Ownership loss has ambiguous external outcome and cannot be retried automatically."]
    Ambiguous,
    #[doc = " Ownership loss is a terminal failure."]
    Fail,
}
#[doc = " Closed work lifecycle."]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorkPhase {
    #[doc = " Waiting for successful dependencies."]
    WaitingDependencies,
    #[doc = " Eligible for deterministic selection."]
    Queued,
    #[doc = " Reserved durably but not yet acknowledged by the worker."]
    Reserved,
    #[doc = " Worker acknowledged execution ownership."]
    Running,
    #[doc = " Retryable failure awaits explicit retry command."]
    RetryPending,
    #[doc = " Cancellation was requested and awaits owner acknowledgement."]
    Cancelling,
    #[doc = " Immutable non-running outcome is retained."]
    Terminal,
}
impl WorkPhase {
    #[doc = " Compares exact lifecycle variants for verified production admission."]
    pub(crate) const fn same(self, other: Self) -> bool {
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
