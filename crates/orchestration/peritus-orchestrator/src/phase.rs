//! Closed E0 lifecycle phases.

/// Resumable nonterminal delivery phase.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ActivePhase {
    /// Writer handoff awaits durable publication.
    WriterPending,
    /// Writer child result is outstanding.
    WriterActive,
    /// Gate cycle awaits durable publication.
    GatesPending,
    /// Gate terminal observation is outstanding.
    GatesActive,
    /// Review handoff awaits durable publication.
    ReviewPending,
    /// Review terminal observation is outstanding.
    ReviewActive,
    /// Fixer handoff awaits durable publication.
    FixerPending,
    /// Fixer child result is outstanding.
    FixerActive,
    /// Checked fixer output awaits atomic candidate advancement.
    RevisionAdvancing,
    /// B2 evaluation is outstanding.
    EvaluatingAcceptance,
    /// Durable B0 acceptance truth is outstanding.
    KernelAcceptancePending,
}

/// Closed aggregate lifecycle including pause, cancellation, and terminal truth.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OrchestratorPhase {
    /// Normal resumable progress.
    Active(ActivePhase),
    /// New work is stopped while the exact phase is retained.
    Paused(ActivePhase),
    /// Cancellation dominates late success until children reconcile.
    Cancelling,
    /// Immutable truthful terminal was committed.
    Terminal,
}

impl OrchestratorPhase {
    /// Returns the active or paused resumable phase.
    #[must_use]
    pub const fn resumable(self) -> Option<ActivePhase> {
        match self {
            Self::Active(phase) | Self::Paused(phase) => Some(phase),
            Self::Cancelling | Self::Terminal => None,
        }
    }

    /// Returns whether the aggregate is terminal.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Terminal)
    }
}
