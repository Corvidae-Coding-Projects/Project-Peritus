//! Provider-neutral failures with transport phase, certainty, and redacted detail.

use core::fmt;

use peritus_types::Sha256Digest;

use crate::{
    ProtocolError, ProtocolErrorKind, ProtocolLimits, ProviderName, RedactedDiagnostic, ResponseId,
};

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

/// Unit or wire form carried by one provider retry-after observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RetryAfterUnit {
    /// A relative count of seconds.
    DeltaSeconds,
    /// An absolute HTTP date.
    HttpDate,
    /// A value with no supported retry-after form.
    Unsupported,
}

/// Result of interpreting one provider retry-after observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RetryAfterParseStatus {
    /// The value was parsed and has a representable schedule.
    Parsed,
    /// The absolute date was already eligible when observed.
    AlreadyEligible,
    /// The value did not match a supported form.
    Invalid,
    /// The form was recognized but its schedule cannot be represented.
    Unrepresentable,
}

/// Bounded evidence for a provider retry-after value.
///
/// Exact bytes are retained when they fit the selected extension bound. Larger values retain
/// their exact byte count and digest so unsupported input is never confused with a missing header.
#[derive(Clone, Eq, PartialEq)]
pub struct RetryAfterObservation {
    raw_value: Option<Vec<u8>>,
    raw_digest: Sha256Digest,
    raw_bytes: u64,
    unit: RetryAfterUnit,
    parse_status: RetryAfterParseStatus,
    eligible_unix_millis: Option<u64>,
}

impl RetryAfterObservation {
    /// Creates a bounded observation from the exact provider header bytes.
    ///
    /// # Errors
    ///
    /// Rejects inconsistent unit, parse-status, or absolute-time combinations.
    pub fn new(
        raw_value: &[u8],
        unit: RetryAfterUnit,
        parse_status: RetryAfterParseStatus,
        eligible_unix_millis: Option<u64>,
        limits: ProtocolLimits,
    ) -> Result<Self, ProtocolError> {
        let raw_bytes = u64::try_from(raw_value.len()).map_err(|_| {
            invalid_retry_after("retry-after byte count is not representable")
        })?;
        let observation = Self {
            raw_value: (raw_value.len() <= limits.max_extension_bytes())
                .then(|| raw_value.to_vec()),
            raw_digest: peritus_codec::sha256(raw_value),
            raw_bytes,
            unit,
            parse_status,
            eligible_unix_millis,
        };
        observation.validate_under(limits)?;
        Ok(observation)
    }

    pub(crate) fn from_encoded(
        raw_value: Option<Vec<u8>>,
        raw_digest: Sha256Digest,
        raw_bytes: u64,
        unit: RetryAfterUnit,
        parse_status: RetryAfterParseStatus,
        eligible_unix_millis: Option<u64>,
        limits: ProtocolLimits,
    ) -> Result<Self, ProtocolError> {
        let observation = Self {
            raw_value,
            raw_digest,
            raw_bytes,
            unit,
            parse_status,
            eligible_unix_millis,
        };
        observation.validate_under(limits)?;
        Ok(observation)
    }

    /// Borrows the exact raw value when it fit the selected durable bound.
    #[must_use]
    pub fn raw_value(&self) -> Option<&[u8]> {
        self.raw_value.as_deref()
    }

    /// Returns the digest of the exact raw value.
    #[must_use]
    pub const fn raw_digest(&self) -> Sha256Digest {
        self.raw_digest
    }

    /// Returns the exact raw byte count.
    #[must_use]
    pub const fn raw_bytes(&self) -> u64 {
        self.raw_bytes
    }

    /// Returns the interpreted unit or wire form.
    #[must_use]
    pub const fn unit(&self) -> RetryAfterUnit {
        self.unit
    }

    /// Returns the parse result.
    #[must_use]
    pub const fn parse_status(&self) -> RetryAfterParseStatus {
        self.parse_status
    }

    /// Returns absolute eligibility for a representable HTTP date.
    #[must_use]
    pub const fn eligible_unix_millis(&self) -> Option<u64> {
        self.eligible_unix_millis
    }

    pub(crate) fn validate_under(&self, limits: ProtocolLimits) -> Result<(), ProtocolError> {
        let shape_valid = matches!(
            (self.unit, self.parse_status, self.eligible_unix_millis),
            (RetryAfterUnit::DeltaSeconds, RetryAfterParseStatus::Parsed, None)
                | (
                    RetryAfterUnit::HttpDate,
                    RetryAfterParseStatus::Parsed | RetryAfterParseStatus::AlreadyEligible,
                    Some(_),
                )
                | (RetryAfterUnit::Unsupported, RetryAfterParseStatus::Invalid, None)
                | (
                    RetryAfterUnit::DeltaSeconds | RetryAfterUnit::HttpDate,
                    RetryAfterParseStatus::Unrepresentable,
                    None,
                )
        );
        if !shape_valid {
            return Err(invalid_retry_after(
                "retry-after unit, parse status, and absolute time are inconsistent",
            ));
        }
        if let Some(raw_value) = &self.raw_value {
            let raw_bytes = u64::try_from(raw_value.len()).map_err(|_| {
                invalid_retry_after("retry-after byte count is not representable")
            })?;
            if raw_value.len() > limits.max_extension_bytes()
                || raw_bytes != self.raw_bytes
                || peritus_codec::sha256(raw_value) != self.raw_digest
            {
                return Err(invalid_retry_after(
                    "retained retry-after bytes exceed bounds or contradict their identity",
                ));
            }
        }
        Ok(())
    }

    fn validate_delay(&self, retry_after_millis: Option<u64>) -> Result<(), ProtocolError> {
        let parsed = matches!(
            self.parse_status,
            RetryAfterParseStatus::Parsed | RetryAfterParseStatus::AlreadyEligible
        );
        if parsed != retry_after_millis.is_some()
            || (self.parse_status == RetryAfterParseStatus::AlreadyEligible
                && retry_after_millis != Some(0))
        {
            return Err(invalid_retry_after(
                "retry-after parse status contradicts its normalized delay",
            ));
        }
        Ok(())
    }
}

impl fmt::Debug for RetryAfterObservation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RetryAfterObservation")
            .field("raw_value_bytes", &self.raw_value.as_ref().map(Vec::len))
            .field("raw_digest", &self.raw_digest)
            .field("raw_bytes", &self.raw_bytes)
            .field("unit", &self.unit)
            .field("parse_status", &self.parse_status)
            .field("eligible_unix_millis", &self.eligible_unix_millis)
            .finish()
    }
}

fn invalid_retry_after(detail: &'static str) -> ProtocolError {
    ProtocolError::at(ProtocolErrorKind::InvalidEvent, "failure.retry_after", detail)
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
    retry_after_observation: Option<RetryAfterObservation>,
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
            retry_after_observation: None,
            diagnostic,
        }
    }

    /// Adds the exact provider scheduling observation behind the normalized delay.
    ///
    /// # Errors
    ///
    /// Rejects a parse status that contradicts the normalized retry-after delay.
    #[must_use]
    pub fn with_retry_after_observation(
        mut self,
        observation: RetryAfterObservation,
    ) -> Result<Self, ProtocolError> {
        observation.validate_delay(self.retry_after_millis)?;
        self.retry_after_observation = Some(observation);
        Ok(self)
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
    /// Exact retry-after parsing evidence when the legacy millisecond field is insufficient.
    #[must_use]
    pub const fn retry_after_observation(&self) -> Option<&RetryAfterObservation> {
        self.retry_after_observation.as_ref()
    }
    /// Redacted allowlisted detail.
    #[must_use]
    pub const fn diagnostic(&self) -> &RedactedDiagnostic {
        &self.diagnostic
    }

    pub(crate) fn validate_under(&self, limits: ProtocolLimits) -> Result<(), ProtocolError> {
        if let Some(observation) = &self.retry_after_observation {
            observation.validate_under(limits)?;
            observation.validate_delay(self.retry_after_millis)?;
        }
        Ok(())
    }
}
