//! Redaction-safe prompt-broker failures.

use core::fmt;

use peritus_app_protocol::PromptError;
use peritus_approval::ApprovalError;

/// Stable category for a rejected prompt-broker operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromptBrokerErrorKind {
    /// A configured prompt count or caller result bound is invalid.
    InvalidLimit,
    /// The bounded registry is full.
    CapacityExceeded,
    /// The exact actor/session-owned correlation set exceeds the caller's result bound.
    ListingLimitExceeded,
    /// An identical prompt registration already exists.
    DuplicateRegistration,
    /// A prompt identity was reused with different binding facts.
    ConflictingRegistration,
    /// No live or retained-terminal prompt has this identity.
    NotFound,
    /// The request does not echo the complete registered correlation.
    BindingMismatch,
    /// The authenticated actor is not the prompt owner.
    ActorMismatch,
    /// The authenticated session is not the prompt owner.
    SessionMismatch,
    /// The authoritative live revision has changed.
    StaleRevision,
    /// The authoritative cancellation generation has changed.
    StaleCancellationGeneration,
    /// The prompt was already cancelled.
    Cancelled,
    /// The exact same terminal response was already accepted.
    DuplicateResponse,
    /// A different terminal response was already accepted.
    ConflictingResponse,
    /// An awaiting prompt cannot yet be retired.
    StillAwaiting,
    /// A3 rejected prompt-local syntax, kind, or constraints.
    Protocol,
    /// An approval challenge is malformed or not bound to the prompt.
    ApprovalChallenge,
    /// Signed approval admission omitted current authority observations.
    ApprovalAuthorityMissing,
    /// The supplied durable credential-registry observation is no longer current for the prompt.
    StaleCredentialRegistry,
    /// The supplied durable authority epoch is no longer the challenged epoch.
    StaleAuthorityEpoch,
    /// B1 rejected canonical decoding, credentials, time, independence, or signature.
    ApprovalAuthentication,
}

/// Redaction-safe rejection preserving structured A3/B1 causes where available.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptBrokerError {
    kind: PromptBrokerErrorKind,
    detail: &'static str,
    prompt: Option<PromptError>,
    approval: Option<ApprovalError>,
}

impl PromptBrokerError {
    pub(super) const fn new(kind: PromptBrokerErrorKind, detail: &'static str) -> Self {
        Self { kind, detail, prompt: None, approval: None }
    }

    pub(super) const fn protocol(error: PromptError) -> Self {
        Self {
            kind: PromptBrokerErrorKind::Protocol,
            detail: "A3 rejected prompt-local admission",
            prompt: Some(error),
            approval: None,
        }
    }

    pub(super) const fn approval(
        kind: PromptBrokerErrorKind,
        detail: &'static str,
        error: ApprovalError,
    ) -> Self {
        Self { kind, detail, prompt: None, approval: Some(error) }
    }

    /// Returns the stable rejection category.
    #[must_use]
    pub const fn kind(&self) -> PromptBrokerErrorKind {
        self.kind
    }

    /// Returns the underlying A3 rejection, when A3 performed the rejected check.
    #[must_use]
    pub const fn prompt_error(&self) -> Option<&PromptError> {
        self.prompt.as_ref()
    }

    /// Returns the underlying B1 rejection, when B1 performed the rejected check.
    #[must_use]
    pub const fn approval_error(&self) -> Option<ApprovalError> {
        self.approval
    }
}

impl fmt::Display for PromptBrokerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.detail)
    }
}

impl std::error::Error for PromptBrokerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.prompt.as_ref().map(|error| error as &(dyn std::error::Error + 'static))
    }
}
