//! Allowlisted, bounded, redaction-aware diagnostics.

use core::fmt;
use std::time::Duration;

use crate::{ProviderCoreError, ProviderCoreErrorKind, RedactedValue, StatusCode};

const MAX_DIAGNOSTIC_VALUE_BYTES: usize = 512;

/// Transport phase in which an observation or failure occurred.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportPhase {
    /// Request construction or validation, before any send.
    BeforeSend,
    /// DNS, TCP, or TLS connection establishment.
    Connecting,
    /// Request headers or body may be in flight.
    Sending,
    /// The request was sent and response headers are pending.
    AwaitingHeaders,
    /// Response-body streaming.
    ReadingBody,
    /// Cancellation-aware retry backoff.
    Backoff,
}

/// A bounded nonsensitive diagnostic value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticValue(String);

impl DiagnosticValue {
    /// Creates a checked value.
    ///
    /// # Errors
    ///
    /// Rejects empty, control-containing, or oversized input.
    pub fn new(value: String) -> Result<Self, ProviderCoreError> {
        if value.is_empty()
            || value.len() > MAX_DIAGNOSTIC_VALUE_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(ProviderCoreError::new(
                ProviderCoreErrorKind::InvalidHttp,
                "diagnostic",
                "diagnostic value is empty, contains controls, or exceeds its byte bound",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the checked value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Redacted diagnostic metadata with a deliberately small field allowlist.
#[derive(Clone, Eq, PartialEq)]
pub struct Diagnostic {
    kind: ProviderCoreErrorKind,
    operation: &'static str,
    phase: TransportPhase,
    status: Option<StatusCode>,
    provider_request_id: Option<RedactedValue>,
    content_type: Option<DiagnosticValue>,
    observed_bytes: u64,
    elapsed: Duration,
}

impl Diagnostic {
    /// Starts a diagnostic from an already redaction-safe error and transport phase.
    #[must_use]
    pub const fn from_error(error: &ProviderCoreError, phase: TransportPhase) -> Self {
        Self {
            kind: error.kind(),
            operation: error.operation(),
            phase,
            status: None,
            provider_request_id: None,
            content_type: None,
            observed_bytes: 0,
            elapsed: Duration::ZERO,
        }
    }

    /// Adds an allowlisted status.
    #[must_use]
    pub const fn with_status(mut self, status: StatusCode) -> Self {
        self.status = Some(status);
        self
    }

    /// Adds a provider request identity whose formatting remains redacted.
    #[must_use]
    pub fn with_provider_request_id(mut self, request_id: RedactedValue) -> Self {
        self.provider_request_id = Some(request_id);
        self
    }

    /// Adds an allowlisted content type.
    #[must_use]
    pub fn with_content_type(mut self, content_type: DiagnosticValue) -> Self {
        self.content_type = Some(content_type);
        self
    }

    /// Adds bounded timing and byte-count observations.
    #[must_use]
    pub const fn with_observation(mut self, observed_bytes: u64, elapsed: Duration) -> Self {
        self.observed_bytes = observed_bytes;
        self.elapsed = elapsed;
        self
    }

    /// Returns the error kind.
    #[must_use]
    pub const fn kind(&self) -> ProviderCoreErrorKind {
        self.kind
    }

    /// Returns the operation.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Returns the phase.
    #[must_use]
    pub const fn phase(&self) -> TransportPhase {
        self.phase
    }

    /// Returns the status, when known.
    #[must_use]
    pub const fn status(&self) -> Option<StatusCode> {
        self.status
    }

    /// Returns the redacted provider request identity, when known.
    #[must_use]
    pub const fn provider_request_id(&self) -> Option<&RedactedValue> {
        self.provider_request_id.as_ref()
    }

    /// Returns the content type, when known.
    #[must_use]
    pub const fn content_type(&self) -> Option<&DiagnosticValue> {
        self.content_type.as_ref()
    }

    /// Returns observed body bytes.
    #[must_use]
    pub const fn observed_bytes(&self) -> u64 {
        self.observed_bytes
    }

    /// Returns elapsed time.
    #[must_use]
    pub const fn elapsed(&self) -> Duration {
        self.elapsed
    }
}

impl fmt::Debug for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Diagnostic")
            .field("kind", &self.kind)
            .field("operation", &self.operation)
            .field("phase", &self.phase)
            .field("status", &self.status)
            .field("provider_request_id", &self.provider_request_id.as_ref().map(|_| "[redacted]"))
            .field("content_type", &self.content_type)
            .field("observed_bytes", &self.observed_bytes)
            .field("elapsed", &self.elapsed)
            .finish()
    }
}
