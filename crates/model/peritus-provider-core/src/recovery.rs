//! Stable provider terminal causes and phase-local recovery dispositions.

use peritus_model_protocol::{FailureCategory, ModelFailure, OutcomeCertainty};

use crate::{ProviderCoreError, ProviderCoreErrorKind};

/// Provider terminal cause normalized independently of any specific wire dialect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderTerminalCause {
    /// Provider returned no usable terminal content.
    EmptyResponse,
    /// Provider terminal payload or framing was malformed.
    MalformedResponse,
    /// Request remained above the provider context envelope after bounded compaction.
    ContextOverflow,
    /// Submission may have been accepted but no safe terminal truth is available.
    AmbiguousAcceptance,
    /// Credential was absent, expired, or rejected.
    Authentication,
    /// Selected model or provider route was temporarily at capacity.
    Capacity,
    /// Temporary provider rate limit.
    RateLimited,
    /// Provider subprocess exceeded its turn deadline.
    SubprocessTimeout,
    /// Caller cancellation won the terminal race.
    Cancelled,
    /// Provider refused the requested output.
    Refusal,
    /// Credential lacks permission for the requested route.
    Permission,
    /// Account/project quota is exhausted.
    QuotaExhausted,
    /// Connection or stream transport failed with safe retry semantics.
    Transport,
    /// Other provider terminal retained without guessing.
    Provider,
}

/// Next legal phase-local action for one normalized terminal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderRecoveryDisposition {
    /// Retry the same route within its existing bounded policy.
    RetrySameRoute,
    /// Try only a separately user-authorized capable fallback route.
    TryAuthorizedFallback,
    /// Perform the bounded context-compaction policy once, then retry the same phase.
    CompactThenRetry,
    /// Pause expensive work until login or credential repair passes a real canary.
    AwaitCredentialRepair,
    /// Stop this phase because a fresh request cannot be justified safely.
    Stop,
}

/// One stable cause paired with its retry disposition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderTerminal {
    cause: ProviderTerminalCause,
    recovery: ProviderRecoveryDisposition,
}

impl ProviderTerminal {
    /// Classifies an empty otherwise-successful provider terminal.
    #[must_use]
    pub const fn empty_response() -> Self {
        Self::new(ProviderTerminalCause::EmptyResponse, ProviderRecoveryDisposition::RetrySameRoute)
    }

    /// Classifies a provider-core setup or transport error.
    #[must_use]
    pub fn from_core_error(error: &ProviderCoreError) -> Self {
        let (cause, recovery) = match error.kind() {
            ProviderCoreErrorKind::InvalidCredential | ProviderCoreErrorKind::Unavailable => (
                ProviderTerminalCause::Authentication,
                ProviderRecoveryDisposition::AwaitCredentialRepair,
            ),
            ProviderCoreErrorKind::LimitExceeded if error.operation() == "context" => (
                ProviderTerminalCause::ContextOverflow,
                ProviderRecoveryDisposition::CompactThenRetry,
            ),
            ProviderCoreErrorKind::MalformedStream => (
                ProviderTerminalCause::MalformedResponse,
                ProviderRecoveryDisposition::RetrySameRoute,
            ),
            ProviderCoreErrorKind::Cancelled => {
                (ProviderTerminalCause::Cancelled, ProviderRecoveryDisposition::Stop)
            }
            ProviderCoreErrorKind::Transport if error.operation() == "process_timeout" => (
                ProviderTerminalCause::SubprocessTimeout,
                ProviderRecoveryDisposition::TryAuthorizedFallback,
            ),
            ProviderCoreErrorKind::Connect => {
                (ProviderTerminalCause::Transport, ProviderRecoveryDisposition::RetrySameRoute)
            }
            ProviderCoreErrorKind::Transport => {
                (ProviderTerminalCause::AmbiguousAcceptance, ProviderRecoveryDisposition::Stop)
            }
            _ => (ProviderTerminalCause::Provider, ProviderRecoveryDisposition::Stop),
        };
        Self::new(cause, recovery)
    }

    /// Classifies a normalized provider terminal event.
    #[must_use]
    pub fn from_model_failure(failure: &ModelFailure) -> Self {
        let diagnostic = failure.diagnostic().code();
        if diagnostic.contains("capacity") || diagnostic.contains("overloaded") {
            return Self::new(
                ProviderTerminalCause::Capacity,
                ProviderRecoveryDisposition::TryAuthorizedFallback,
            );
        }
        if diagnostic.contains("context_limit") || diagnostic.contains("context_overflow") {
            return Self::new(
                ProviderTerminalCause::ContextOverflow,
                ProviderRecoveryDisposition::CompactThenRetry,
            );
        }
        let (cause, recovery) = match failure.category() {
            FailureCategory::Authentication => (
                ProviderTerminalCause::Authentication,
                ProviderRecoveryDisposition::AwaitCredentialRepair,
            ),
            FailureCategory::Permission => {
                (ProviderTerminalCause::Permission, ProviderRecoveryDisposition::Stop)
            }
            FailureCategory::RateLimited => {
                (ProviderTerminalCause::RateLimited, ProviderRecoveryDisposition::RetrySameRoute)
            }
            FailureCategory::QuotaExhausted => (
                ProviderTerminalCause::QuotaExhausted,
                ProviderRecoveryDisposition::TryAuthorizedFallback,
            ),
            FailureCategory::TransientProvider => (
                ProviderTerminalCause::Capacity,
                ProviderRecoveryDisposition::TryAuthorizedFallback,
            ),
            FailureCategory::MalformedPayload | FailureCategory::IncompleteStream => (
                ProviderTerminalCause::MalformedResponse,
                ProviderRecoveryDisposition::RetrySameRoute,
            ),
            FailureCategory::AmbiguousAcceptance => {
                (ProviderTerminalCause::AmbiguousAcceptance, ProviderRecoveryDisposition::Stop)
            }
            FailureCategory::Timeout => (
                ProviderTerminalCause::SubprocessTimeout,
                ProviderRecoveryDisposition::TryAuthorizedFallback,
            ),
            FailureCategory::Cancellation => {
                (ProviderTerminalCause::Cancelled, ProviderRecoveryDisposition::Stop)
            }
            FailureCategory::Refusal | FailureCategory::Safety => {
                (ProviderTerminalCause::Refusal, ProviderRecoveryDisposition::Stop)
            }
            FailureCategory::Transport
                if failure.certainty() != OutcomeCertainty::DefinitelyNotAccepted =>
            {
                (ProviderTerminalCause::AmbiguousAcceptance, ProviderRecoveryDisposition::Stop)
            }
            FailureCategory::Transport => {
                (ProviderTerminalCause::Transport, ProviderRecoveryDisposition::RetrySameRoute)
            }
            _ => (ProviderTerminalCause::Provider, ProviderRecoveryDisposition::Stop),
        };
        Self::new(cause, recovery)
    }

    /// Stable normalized cause.
    #[must_use]
    pub const fn cause(self) -> ProviderTerminalCause {
        self.cause
    }

    /// Legal next action for this cause.
    #[must_use]
    pub const fn recovery(self) -> ProviderRecoveryDisposition {
        self.recovery
    }

    const fn new(cause: ProviderTerminalCause, recovery: ProviderRecoveryDisposition) -> Self {
        Self { cause, recovery }
    }
}
