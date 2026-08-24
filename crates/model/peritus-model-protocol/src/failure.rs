//! Provider-neutral failures with transport phase, certainty, and redacted detail.

use crate::{ProviderName, RedactedDiagnostic, ResponseId};

/// Stable failure taxonomy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum FailureCategory {
    /// Request failed local validation.
    InvalidRequest,
    /// Credential was absent or rejected.
    Authentication,
    /// Credential lacks provider permission.
    Permission,
    /// Model/resource was not found.
    NotFound,
    /// Temporary provider rate limit.
    RateLimited,
    /// Account/project quota or billing exhaustion.
    QuotaExhausted,
    /// Provider transient/unavailable failure.
    TransientProvider,
    /// Network/TLS/HTTP transport failure.
    Transport,
    /// Request may have been accepted without a terminal result.
    AmbiguousAcceptance,
    /// Provider payload or normalized event grammar was malformed.
    MalformedPayload,
    /// Stream ended without a required terminal.
    IncompleteStream,
    /// Deadline elapsed.
    Timeout,
    /// Explicit provider refusal.
    Refusal,
    /// Provider safety policy prevented output.
    Safety,
    /// Local cancellation.
    Cancellation,
    /// Unknown provider failure retained without guessing.
    Provider,
}

/// Furthest transport/request phase observed.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TransportPhase {
    /// No bytes were submitted.
    BeforeSend,
    /// DNS/TCP/TLS connection setup.
    Connecting,
    /// Request headers were being sent.
    SendingHeaders,
    /// Request body may have been sent.
    SendingBody,
    /// Request sent; waiting for response headers.
    AwaitingHeaders,
    /// Response headers accepted, no application event emitted.
    ReadingBody,
    /// At least one application-visible response event was emitted.
    StreamObserved,
    /// Provider emitted a terminal outcome.
    Completed,
}

/// Certainty of server-side acceptance/effects.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutcomeCertainty {
    /// Request is known not to have been accepted.
    DefinitelyNotAccepted,
    /// Request may have been accepted and may incur output/cost.
    MaybeAccepted,
    /// Provider accepted the request and emitted partial output.
    AcceptedPartial,
    /// Provider emitted an explicit terminal.
    Terminal,
}

/// Normalized retry safety observation, not permission to retry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Retryability {
    /// Retry cannot be justified.
    Never,
    /// A fresh request is safe under bounded retry policy.
    SafeNewRequest,
    /// Only exact cursor resumption is safe.
    ExactResumeOnly,
    /// Ambiguous behavior requires explicit caller policy.
    CallerDecision,
}

/// Complete redacted model-provider failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelFailure {
    provider: ProviderName,
    category: FailureCategory,
    phase: TransportPhase,
    certainty: OutcomeCertainty,
    retryability: Retryability,
    http_status: Option<u16>,
    response_id: Option<ResponseId>,
    retry_after_millis: Option<u64>,
    diagnostic: RedactedDiagnostic,
}

impl ModelFailure {
    /// Creates a fully classified redacted failure.
    #[allow(
        clippy::too_many_arguments,
        reason = "failure binds every independent safety observation"
    )]
    #[must_use]
    pub const fn new(
        provider: ProviderName,
        category: FailureCategory,
        phase: TransportPhase,
        certainty: OutcomeCertainty,
        retryability: Retryability,
        http_status: Option<u16>,
        response_id: Option<ResponseId>,
        retry_after_millis: Option<u64>,
        diagnostic: RedactedDiagnostic,
    ) -> Self {
        Self {
            provider,
            category,
            phase,
            certainty,
            retryability,
            http_status,
            response_id,
            retry_after_millis,
            diagnostic,
        }
    }

    /// Provider family.
    #[must_use]
    pub const fn provider(&self) -> &ProviderName {
        &self.provider
    }
    /// Stable category.
    #[must_use]
    pub const fn category(&self) -> FailureCategory {
        self.category
    }
    /// Furthest transport phase.
    #[must_use]
    pub const fn phase(&self) -> TransportPhase {
        self.phase
    }
    /// Acceptance certainty.
    #[must_use]
    pub const fn certainty(&self) -> OutcomeCertainty {
        self.certainty
    }
    /// Retry safety observation.
    #[must_use]
    pub const fn retryability(&self) -> Retryability {
        self.retryability
    }
    /// HTTP status if a response was received.
    #[must_use]
    pub const fn http_status(&self) -> Option<u16> {
        self.http_status
    }
    /// Sensitive provider response identity.
    #[must_use]
    pub const fn response_id(&self) -> Option<&ResponseId> {
        self.response_id.as_ref()
    }
    /// Provider retry-after delay.
    #[must_use]
    pub const fn retry_after_millis(&self) -> Option<u64> {
        self.retry_after_millis
    }
    /// Redacted allowlisted detail.
    #[must_use]
    pub const fn diagnostic(&self) -> &RedactedDiagnostic {
        &self.diagnostic
    }
}
