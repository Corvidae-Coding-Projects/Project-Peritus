//! Closed agent-turn lifecycle phases.

use vstd::prelude::*;

verus! {

/// Resumable non-terminal phase.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ActivePhase {
    PreparingContext,
    RequestingModel,
    StreamingResponse,
    ProposedToolCalls,
    AwaitingAuthorization,
    ExecutingTools,
    RecordingResults,
    ProposedCompletion,
}

/// Terminal outcome. Completion means only that a proposal was durably emitted.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TerminalKind {
    Completed,
    Failed,
    Cancelled,
}

/// Complete aggregate phase, including explicit pause and cancellation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AgentPhase {
    Active(ActivePhase),
    Paused,
    Cancelling,
    Terminal(TerminalKind),
}

impl AgentPhase {
    #[must_use]
    pub const fn is_terminal(self) -> (terminal: bool)
        ensures terminal == matches!(self, Self::Terminal(_)),
    {
        matches!(self, Self::Terminal(_))
    }

    /// Stable wire/digest tag for this exact phase.
    pub open spec fn tag_spec(self) -> u8 {
        match self {
            Self::Active(ActivePhase::PreparingContext) => 0,
            Self::Active(ActivePhase::RequestingModel) => 1,
            Self::Active(ActivePhase::StreamingResponse) => 2,
            Self::Active(ActivePhase::ProposedToolCalls) => 3,
            Self::Active(ActivePhase::AwaitingAuthorization) => 4,
            Self::Active(ActivePhase::ExecutingTools) => 5,
            Self::Active(ActivePhase::RecordingResults) => 6,
            Self::Active(ActivePhase::ProposedCompletion) => 7,
            Self::Paused => 8,
            Self::Cancelling => 9,
            Self::Terminal(TerminalKind::Completed) => 10,
            Self::Terminal(TerminalKind::Failed) => 11,
            Self::Terminal(TerminalKind::Cancelled) => 12,
        }
    }

    #[must_use]
    pub const fn tag(self) -> (tag: u8)
        ensures tag == self.tag_spec(),
    {
        match self {
            Self::Active(ActivePhase::PreparingContext) => 0,
            Self::Active(ActivePhase::RequestingModel) => 1,
            Self::Active(ActivePhase::StreamingResponse) => 2,
            Self::Active(ActivePhase::ProposedToolCalls) => 3,
            Self::Active(ActivePhase::AwaitingAuthorization) => 4,
            Self::Active(ActivePhase::ExecutingTools) => 5,
            Self::Active(ActivePhase::RecordingResults) => 6,
            Self::Active(ActivePhase::ProposedCompletion) => 7,
            Self::Paused => 8,
            Self::Cancelling => 9,
            Self::Terminal(TerminalKind::Completed) => 10,
            Self::Terminal(TerminalKind::Failed) => 11,
            Self::Terminal(TerminalKind::Cancelled) => 12,
        }
    }
}

} // verus!
