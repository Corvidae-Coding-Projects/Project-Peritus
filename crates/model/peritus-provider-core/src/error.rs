//! Stable redaction-safe provider-core errors.

use core::fmt;

/// Stable category for a provider-core failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProviderCoreErrorKind {
    /// An endpoint failed validation.
    InvalidEndpoint,
    /// A credential reference or value failed validation or resolution.
    InvalidCredential,
    /// A model request did not match the provider's immutable profile.
    InvalidRequest,
    /// An HTTP value or operation was invalid.
    InvalidHttp,
    /// A configured or observed resource limit was exceeded.
    LimitExceeded,
    /// The caller cancelled the operation.
    Cancelled,
    /// Connection or TLS establishment failed before request submission.
    Connect,
    /// The HTTP transport failed.
    Transport,
    /// A streaming frame was malformed or incomplete.
    MalformedStream,
    /// Retry inputs or a retry plan were invalid.
    InvalidRetry,
    /// The production transport could not be configured.
    Configuration,
    /// The route cannot satisfy the declared role capability envelope.
    UnsupportedCapability,
    /// The route has no currently usable credential or verified account session.
    Unavailable,
}

impl ProviderCoreErrorKind {
    /// Returns the stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidEndpoint => "PERITUS-PROVIDER-CORE-001",
            Self::InvalidCredential => "PERITUS-PROVIDER-CORE-002",
            Self::InvalidRequest => "PERITUS-PROVIDER-CORE-010",
            Self::InvalidHttp => "PERITUS-PROVIDER-CORE-003",
            Self::LimitExceeded => "PERITUS-PROVIDER-CORE-004",
            Self::Cancelled => "PERITUS-PROVIDER-CORE-005",
            Self::Connect => "PERITUS-PROVIDER-CORE-011",
            Self::Transport => "PERITUS-PROVIDER-CORE-006",
            Self::MalformedStream => "PERITUS-PROVIDER-CORE-007",
            Self::InvalidRetry => "PERITUS-PROVIDER-CORE-008",
            Self::Configuration => "PERITUS-PROVIDER-CORE-009",
            Self::UnsupportedCapability => "PERITUS-PROVIDER-CORE-012",
            Self::Unavailable => "PERITUS-PROVIDER-CORE-013",
        }
    }
}

/// Bounded failure containing only stable, static, redaction-safe text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderCoreError {
    kind: ProviderCoreErrorKind,
    operation: &'static str,
    detail: &'static str,
}

impl ProviderCoreError {
    pub(crate) const fn new(
        kind: ProviderCoreErrorKind,
        operation: &'static str,
        detail: &'static str,
    ) -> Self {
        Self { kind, operation, detail }
    }

    /// Creates a redaction-safe error for a custom transport implementation.
    #[must_use]
    pub const fn transport(operation: &'static str, detail: &'static str) -> Self {
        Self::new(ProviderCoreErrorKind::Transport, operation, detail)
    }

    /// Creates a redaction-safe pre-submission connection or TLS error.
    #[must_use]
    pub const fn connect(operation: &'static str, detail: &'static str) -> Self {
        Self::new(ProviderCoreErrorKind::Connect, operation, detail)
    }

    /// Creates a redaction-safe credential-source error.
    #[must_use]
    pub const fn credential(detail: &'static str) -> Self {
        Self::new(ProviderCoreErrorKind::InvalidCredential, "credential_source", detail)
    }

    /// Creates a cancellation error for an interrupted operation.
    #[must_use]
    pub const fn cancelled(operation: &'static str) -> Self {
        Self::new(ProviderCoreErrorKind::Cancelled, operation, "operation cancelled")
    }

    /// Creates a redaction-safe request-validation error for a provider adapter.
    #[must_use]
    pub const fn invalid_request(operation: &'static str, detail: &'static str) -> Self {
        Self::new(ProviderCoreErrorKind::InvalidRequest, operation, detail)
    }

    /// Creates a redaction-safe resource-limit error for a provider adapter.
    #[must_use]
    pub const fn limit_exceeded(operation: &'static str, detail: &'static str) -> Self {
        Self::new(ProviderCoreErrorKind::LimitExceeded, operation, detail)
    }

    /// Creates a redaction-safe malformed-stream error for a provider adapter.
    #[must_use]
    pub const fn malformed_stream(operation: &'static str, detail: &'static str) -> Self {
        Self::new(ProviderCoreErrorKind::MalformedStream, operation, detail)
    }

    /// Creates a redaction-safe configuration error for a provider adapter.
    #[must_use]
    pub const fn configuration(operation: &'static str, detail: &'static str) -> Self {
        Self::new(ProviderCoreErrorKind::Configuration, operation, detail)
    }

    /// Creates a redaction-safe route-capability mismatch.
    #[must_use]
    pub const fn unsupported_capability(detail: &'static str) -> Self {
        Self::new(ProviderCoreErrorKind::UnsupportedCapability, "provider_qualification", detail)
    }

    /// Creates a redaction-safe provider-availability failure.
    #[must_use]
    pub const fn unavailable(detail: &'static str) -> Self {
        Self::new(ProviderCoreErrorKind::Unavailable, "provider_qualification", detail)
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> ProviderCoreErrorKind {
        self.kind
    }

    /// Returns the stable failure code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.kind.code()
    }

    /// Returns the static operation name.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Returns the static redaction-safe detail.
    #[must_use]
    pub const fn detail(&self) -> &'static str {
        self.detail
    }
}

impl fmt::Display for ProviderCoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}: {}", self.code(), self.operation, self.detail)
    }
}

impl std::error::Error for ProviderCoreError {}
