//! Stable failures returned by pure validation, reduction, and replay.

use crate::AgentPhase;
use peritus_types::TurnId;
use std::fmt::{Display, Formatter};

/// Stable machine-readable agent failure category.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AgentErrorCode {
    InvalidBinding,
    InvalidLimit,
    InvalidText,
    NonCanonicalOrder,
    RevisionMismatch,
    CausalMismatch,
    IllegalPhase,
    InvalidCommand,
    InvalidTool,
    InvalidProgress,
    LimitExceeded,
    ArithmeticOverflow,
    CompletionIneligible,
    ReplayMismatch,
    IndeterminateEffect,
}

/// Operation that failed without revealing sensitive payloads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentOperation {
    ValidateBinding,
    ValidateLimits,
    ValidateCompletion,
    ValidateTools,
    Start,
    Reduce,
    Replay,
}

/// Stable caller recovery classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentRecovery {
    CorrectRequest,
    RetrySameCommand,
    ResumeProvider,
    ReconcileTool,
    RestartTurn,
    RequestAuthority,
    Exhausted,
    Terminal,
    Indeterminate,
}

/// Bounded, secret-safe rejection. The aggregate is unchanged when this is returned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AgentRejection {
    code: AgentErrorCode,
    operation: AgentOperation,
    recovery: AgentRecovery,
    detail: &'static str,
    turn_id: Option<TurnId>,
    phase: Option<AgentPhase>,
}

impl AgentRejection {
    pub(crate) const fn new(
        code: AgentErrorCode,
        operation: AgentOperation,
        recovery: AgentRecovery,
        detail: &'static str,
    ) -> Self {
        Self { code, operation, recovery, detail, turn_id: None, phase: None }
    }

    pub(crate) const fn at(mut self, turn_id: TurnId, phase: AgentPhase) -> Self {
        self.turn_id = Some(turn_id);
        self.phase = Some(phase);
        self
    }

    #[must_use]
    pub const fn code(self) -> AgentErrorCode {
        self.code
    }
    #[must_use]
    pub const fn operation(self) -> AgentOperation {
        self.operation
    }
    #[must_use]
    pub const fn recovery(self) -> AgentRecovery {
        self.recovery
    }
    #[must_use]
    pub const fn detail(self) -> &'static str {
        self.detail
    }
    #[must_use]
    pub const fn turn_id(self) -> Option<TurnId> {
        self.turn_id
    }
    #[must_use]
    pub const fn phase(self) -> Option<AgentPhase> {
        self.phase
    }
}

impl Display for AgentRejection {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "agent {:?} failed: {}", self.operation, self.detail)
    }
}

impl std::error::Error for AgentRejection {}
