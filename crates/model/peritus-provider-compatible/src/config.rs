//! Exact endpoint, header authentication, fixed headers, limits, and retry configuration.

mod mappings;

pub use mappings::{
    CompatibleRateHeaders, CompatibleResetUnit, CompatibleResponseHeaders, CompatibleRetryStatuses,
};

use core::fmt;
use std::collections::BTreeSet;
use std::time::Duration;

use peritus_model_protocol::ProtocolLimits;
use peritus_provider_core::{
    Credential, CredentialReference, Endpoint, FramingLimits, Header, HeaderName, HttpLimits,
    ProviderCoreError, RetryPolicy,
};

use crate::error;

const MAX_FIXED_HEADERS: usize = 16;
const MAX_FIXED_HEADER_VALUE_BYTES: usize = 4_096;

/// Credential value projection for a compatible endpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialScheme {
    /// `Authorization: Bearer <credential>`.
    Bearer,
    /// Raw credential bytes in one explicitly named non-Authorization header.
    RawHeader,
}

/// Header-only credential placement for one configured endpoint.
#[derive(Clone, Eq, PartialEq)]
pub struct CompatibleAuth {
    credential: CredentialReference,
    header: HeaderName,
    scheme: CredentialScheme,
}

impl CompatibleAuth {
    /// Creates standard bearer authentication in the `Authorization` header.
    ///
    /// # Errors
    ///
    /// Returns an error only if the audited static Authorization name ceases to be valid.
    pub fn bearer(credential: CredentialReference) -> Result<Self, ProviderCoreError> {
        Ok(Self {
            credential,
            header: HeaderName::new("authorization".to_owned())?,
            scheme: CredentialScheme::Bearer,
        })
    }

    /// Creates raw credential authentication in one explicit custom header.
    ///
    /// # Errors
    ///
    /// Rejects Authorization, routing, connection-controlled, or non-secret-looking names.
    pub fn raw_header(
        credential: CredentialReference,
        header: HeaderName,
    ) -> Result<Self, ProviderCoreError> {
        let name = header.as_str();
        if name == "authorization" || reserved_header(name) || !secret_header_name(name) {
            return Err(error::configuration(
                "raw compatible authentication requires an explicit safe API-key header",
            ));
        }
        Ok(Self { credential, header, scheme: CredentialScheme::RawHeader })
    }

    /// Returns the opaque credential reference.
    #[must_use]
    pub const fn credential(&self) -> &CredentialReference {
        &self.credential
    }

    /// Returns the exact authentication header name.
    #[must_use]
    pub const fn header(&self) -> &HeaderName {
        &self.header
    }

    /// Returns the exact credential value scheme.
    #[must_use]
    pub const fn scheme(&self) -> CredentialScheme {
        self.scheme
    }

    pub(crate) fn project(&self, credential: Credential) -> Result<Header, ProviderCoreError> {
        let prefix = match self.scheme {
            CredentialScheme::Bearer => Some("Bearer "),
            CredentialScheme::RawHeader => None,
        };
        credential.into_header(self.header.clone(), prefix)
    }
}

impl fmt::Debug for CompatibleAuth {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CompatibleAuth")
            .field("credential", &self.credential)
            .field("header", &self.header)
            .field("scheme", &self.scheme)
            .finish()
    }
}

/// One fixed nonsensitive request header.
#[derive(Clone, Eq, PartialEq)]
pub struct CompatibleHeader {
    name: HeaderName,
    value: Vec<u8>,
}

impl CompatibleHeader {
    /// Creates one bounded fixed header that cannot carry credentials or control routing.
    ///
    /// # Errors
    ///
    /// Rejects reserved, secret-bearing, oversized, or control-containing values.
    pub fn new(name: HeaderName, value: String) -> Result<Self, ProviderCoreError> {
        if reserved_header(name.as_str())
            || secret_header_name(name.as_str())
            || value.is_empty()
            || value.len() > MAX_FIXED_HEADER_VALUE_BYTES
            || value.bytes().any(|byte| byte < 0x20 || byte == 0x7f)
        {
            return Err(error::configuration(
                "fixed compatible header is reserved, sensitive, malformed, or oversized",
            ));
        }
        Ok(Self { name, value: value.into_bytes() })
    }

    /// Returns the exact fixed header name.
    #[must_use]
    pub const fn name(&self) -> &HeaderName {
        &self.name
    }

    pub(crate) fn project(&self) -> Result<Header, ProviderCoreError> {
        Header::new(self.name.clone(), self.value.clone())
    }
}

impl fmt::Debug for CompatibleHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CompatibleHeader")
            .field("name", &self.name)
            .field("value_bytes", &self.value.len())
            .finish()
    }
}

/// Exact compatible endpoint and transport policy.
#[derive(Clone)]
pub struct CompatibleConfig {
    endpoint: Endpoint,
    auth: CompatibleAuth,
    fixed_headers: Vec<CompatibleHeader>,
    response_headers: CompatibleResponseHeaders,
    retry_statuses: CompatibleRetryStatuses,
    retry_policy: RetryPolicy,
    http_limits: HttpLimits,
    framing_limits: FramingLimits,
    protocol_limits: ProtocolLimits,
}

impl CompatibleConfig {
    /// Creates minimum-safe configuration for one exact non-root endpoint URL.
    ///
    /// The supplied [`Endpoint`] retains its exact path and fixed nonsensitive query. Secret query
    /// names are already rejected by the provider-core endpoint boundary.
    ///
    /// # Errors
    ///
    /// Rejects an endpoint without an explicit operation path.
    pub fn new(endpoint: Endpoint, auth: CompatibleAuth) -> Result<Self, ProviderCoreError> {
        if operation_path(endpoint.as_str()).is_none() {
            return Err(error::configuration(
                "compatible endpoint must include one exact non-root operation path",
            ));
        }
        Ok(Self {
            endpoint,
            auth,
            fixed_headers: Vec::new(),
            response_headers: CompatibleResponseHeaders::none(),
            retry_statuses: CompatibleRetryStatuses::none(),
            retry_policy: RetryPolicy::new(
                3,
                [
                    Duration::from_millis(100),
                    Duration::from_secs(2),
                    Duration::from_secs(2),
                    Duration::from_secs(10),
                ],
                64 * 1024 * 1024,
            )?,
            http_limits: HttpLimits::PRODUCTION,
            framing_limits: FramingLimits::PRODUCTION,
            protocol_limits: ProtocolLimits::PRODUCTION,
        })
    }

    /// Installs an exact bounded set of nonsensitive fixed headers.
    ///
    /// # Errors
    ///
    /// Rejects duplicate names or a count outside the compatible contract bound.
    pub fn with_fixed_headers(
        mut self,
        headers: Vec<CompatibleHeader>,
    ) -> Result<Self, ProviderCoreError> {
        let mut names = BTreeSet::new();
        if headers.len() > MAX_FIXED_HEADERS
            || headers.iter().any(|header| !names.insert(header.name.as_str()))
        {
            return Err(error::configuration(
                "fixed compatible headers are duplicated or exceed their count bound",
            ));
        }
        self.fixed_headers = headers;
        Ok(self)
    }

    /// Replaces the bounded retry policy.
    #[must_use]
    pub const fn with_retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    /// Installs exact documented response-header mappings.
    #[must_use]
    pub fn with_response_headers(mut self, mappings: CompatibleResponseHeaders) -> Self {
        self.response_headers = mappings;
        self
    }

    /// Installs explicit temporary rejection classes. This does not claim create idempotency.
    #[must_use]
    pub const fn with_retry_statuses(mut self, statuses: CompatibleRetryStatuses) -> Self {
        self.retry_statuses = statuses;
        self
    }

    /// Returns the exact operation endpoint, including fixed safe query parameters.
    #[must_use]
    pub const fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Returns the exact header authentication contract.
    #[must_use]
    pub const fn auth(&self) -> &CompatibleAuth {
        &self.auth
    }

    /// Returns fixed nonsensitive headers.
    #[must_use]
    pub fn fixed_headers(&self) -> &[CompatibleHeader] {
        &self.fixed_headers
    }

    /// Returns the exact provider-specific response-header mappings.
    #[must_use]
    pub const fn response_headers(&self) -> &CompatibleResponseHeaders {
        &self.response_headers
    }

    /// Returns explicitly retryable non-accepting HTTP statuses.
    #[must_use]
    pub const fn retry_statuses(&self) -> CompatibleRetryStatuses {
        self.retry_statuses
    }

    /// Returns the bounded retry policy.
    #[must_use]
    pub const fn retry_policy(&self) -> RetryPolicy {
        self.retry_policy
    }

    /// Returns HTTP request, response, and header limits.
    #[must_use]
    pub const fn http_limits(&self) -> HttpLimits {
        self.http_limits
    }

    /// Returns SSE framing limits.
    #[must_use]
    pub const fn framing_limits(&self) -> FramingLimits {
        self.framing_limits
    }

    /// Returns provider-neutral event and output limits.
    #[must_use]
    pub const fn protocol_limits(&self) -> ProtocolLimits {
        self.protocol_limits
    }
}

impl fmt::Debug for CompatibleConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CompatibleConfig")
            .field("endpoint", &self.endpoint)
            .field("auth", &self.auth)
            .field("fixed_headers", &self.fixed_headers)
            .field("response_headers", &self.response_headers)
            .field("retry_statuses", &self.retry_statuses)
            .field("retry_policy", &self.retry_policy)
            .field("http_limits", &self.http_limits)
            .field("framing_limits", &self.framing_limits)
            .field("protocol_limits", &self.protocol_limits)
            .finish()
    }
}

fn operation_path(endpoint: &str) -> Option<&str> {
    let authority = endpoint.split_once("://")?.1;
    let path = authority.find('/').map(|index| &authority[index..])?;
    let path = path.split('?').next().unwrap_or(path).trim_end_matches('/');
    (!path.is_empty()).then_some(path)
}

pub fn secret_header_name(name: &str) -> bool {
    ["authorization", "cookie", "credential", "secret", "token", "api-key", "api_key"]
        .iter()
        .any(|marker| name.contains(marker))
}

pub fn reserved_header(name: &str) -> bool {
    name.starts_with("proxy-")
        || matches!(
            name,
            "accept"
                | "connection"
                | "content-length"
                | "content-type"
                | "host"
                | "keep-alive"
                | "te"
                | "trailer"
                | "transfer-encoding"
                | "upgrade"
        )
}
