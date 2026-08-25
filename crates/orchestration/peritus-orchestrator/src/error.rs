//! Stable typed E0 failures and operator recovery actions.

use core::fmt;

/// Closed failure classification exposed by the E0 boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrchestratorErrorKind {
    /// A checked constructor rejected malformed or incomplete caller input.
    InvalidInput,
    /// The requested command is illegal in the current phase.
    InvalidTransition,
    /// A command fence names an earlier aggregate state.
    StaleState,
    /// An actor, role, candidate, revision, or child reference does not match.
    BindingMismatch,
    /// A compiled or contract completion bound would be exceeded.
    LimitExceeded,
    /// An ordered collection or encoded value is not canonical.
    NonCanonical,
    /// A protocol frame is malformed, unsupported, or contains trailing bytes.
    Codec,
    /// A command identity was reused with conflicting canonical bytes.
    Conflict,
    /// A required complete checkpoint is absent.
    MissingCheckpoint,
    /// Durable bytes, replayed state, or causal history disagree.
    Integrity,
    /// A referenced child cannot be reconciled automatically.
    ChildAmbiguous,
    /// A journal, outbox, child, evaluation, or kernel port failed.
    External,
}

/// Stable recovery classification for one E0 failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrchestratorRecoveryAction {
    /// Correct the inert command, observation, or constructor input.
    CorrectInput,
    /// Reload and replay the authoritative aggregate.
    Replay,
    /// Load and reconcile the exact referenced child heads.
    ReconcileChild,
    /// Retry publication of the already committed idempotent directive.
    RetryDelivery,
    /// Obtain human judgment or external authority.
    NeedsHuman,
    /// Preserve the store and stop automatic progress for diagnosis.
    Quarantine,
}

/// One bounded typed E0 error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OrchestratorError {
    kind: OrchestratorErrorKind,
    recovery: OrchestratorRecoveryAction,
    detail: &'static str,
}

impl OrchestratorError {
    /// Creates a stable typed error without retaining untrusted payload data.
    #[must_use]
    pub const fn new(
        kind: OrchestratorErrorKind,
        recovery: OrchestratorRecoveryAction,
        detail: &'static str,
    ) -> Self {
        Self { kind, recovery, detail }
    }

    /// Returns the closed failure kind.
    #[must_use]
    pub const fn kind(self) -> OrchestratorErrorKind {
        self.kind
    }

    /// Returns the required operator recovery action.
    #[must_use]
    pub const fn recovery(self) -> OrchestratorRecoveryAction {
        self.recovery
    }

    /// Returns bounded diagnostic detail with no external payload content.
    #[must_use]
    pub const fn detail(self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for OrchestratorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.detail)
    }
}

impl std::error::Error for OrchestratorError {}
