//! Normalized provider finish reasons and exactly-one terminal outcomes.

use crate::{BoundedText, ModelFailure, ProtocolVersion};

/// Provider-neutral finish reason with bounded raw fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FinishReason {
    /// Natural stop/end turn.
    Stop,
    /// Output token/length limit.
    Length,
    /// One or more application tools require results.
    ToolCalls,
    /// Safety/content filtering prevented normal completion.
    Safety,
    /// Model refusal.
    Refusal,
    /// Provider paused and requires semantic continuation.
    Pause,
    /// Context-window limit.
    ContextLimit,
    /// Provider reported cancellation.
    Cancelled,
    /// Provider reported incomplete output for another known reason.
    Incomplete,
    /// Unknown provider value retained without assuming success.
    Provider(BoundedText),
}

/// Final reducer outcome. Exactly one value may be established.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TerminalOutcome {
    /// Complete successful model output.
    Succeeded {
        /// Provider finish reason establishing success.
        reason: FinishReason,
    },
    /// Complete tool-call/pause output that requires caller action or continuation.
    RequiresAction {
        /// Tool-call or continuation reason.
        reason: FinishReason,
    },
    /// Explicit model refusal/safety terminal.
    Refused {
        /// Refusal or safety reason.
        reason: FinishReason,
    },
    /// Explicit incomplete terminal.
    Incomplete {
        /// Explicit incomplete reason.
        reason: FinishReason,
    },
    /// Explicit cancellation terminal.
    Cancelled,
    /// Typed failure terminal.
    Failed(ModelFailure),
}

impl TerminalOutcome {
    /// Protocol version governing this terminal outcome.
    #[must_use]
    pub const fn protocol(&self) -> ProtocolVersion {
        ProtocolVersion::V1
    }
}
