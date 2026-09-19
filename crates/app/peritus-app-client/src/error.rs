//! Interface-independent transport failures.

use core::fmt;

use peritus_app_protocol::AppProtocolError;

/// Stable classification for presentation and recovery decisions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientErrorKind {
    /// The local endpoint could not be reached or stopped responding.
    Connection,
    /// The peer did not establish a compatible application session.
    Negotiation,
    /// A frame or protocol value violated the negotiated contract.
    Protocol,
    /// The platform cannot provide a required local capability.
    Unsupported,
    /// The operating system could not generate an operation identity.
    Identity,
}

/// A transport failure does not establish whether a submitted effect occurred.
///
/// Callers must reconcile the original operation receipt before retrying an
/// authority-bearing request whose response was lost.
#[derive(Debug)]
pub struct ClientError {
    kind: ClientErrorKind,
    operation: &'static str,
    detail: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl ClientError {
    /// Returns the stable failure category.
    #[must_use]
    pub const fn kind(&self) -> ClientErrorKind {
        self.kind
    }

    /// Returns the operation being attempted when the failure was observed.
    #[must_use]
    pub const fn operation(&self) -> &'static str {
        self.operation
    }

    /// Returns a diagnostic without endpoint credentials or request payloads.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    pub(crate) fn new(
        kind: ClientErrorKind,
        operation: &'static str,
        detail: impl Into<String>,
    ) -> Self {
        Self { kind, operation, detail: detail.into(), source: None }
    }

    pub(crate) fn connection(operation: &'static str, detail: impl Into<String>) -> Self {
        Self::new(ClientErrorKind::Connection, operation, detail)
    }

    pub(crate) fn negotiation(detail: impl Into<String>) -> Self {
        Self::new(ClientErrorKind::Negotiation, "negotiate daemon session", detail)
    }

    pub(crate) fn protocol(operation: &'static str, detail: impl Into<String>) -> Self {
        Self::new(ClientErrorKind::Protocol, operation, detail)
    }
}

impl fmt::Display for ClientError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.operation, self.detail)
    }
}

impl std::error::Error for ClientError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|source| source as &(dyn std::error::Error + 'static))
    }
}

impl From<peritus_types::IdentifierError> for ClientError {
    fn from(error: peritus_types::IdentifierError) -> Self {
        Self::new(
            ClientErrorKind::Identity,
            "construct application identity",
            format!("invalid protocol identifier: {error:?}"),
        )
    }
}

impl From<AppProtocolError> for ClientError {
    fn from(error: AppProtocolError) -> Self {
        Self {
            kind: ClientErrorKind::Protocol,
            operation: "validate application protocol",
            detail: error.to_string(),
            source: Some(Box::new(error)),
        }
    }
}
