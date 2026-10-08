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
    ProviderCoreError, RetryPolicy, catalog::derive_compatible_catalog_endpoint,
};

use crate::error;

const MANDATORY_REQUEST_HEADERS: usize = 3;

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
            || value.len() > HttpLimits::PRODUCTION.max_header_bytes()
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

/// Deliberately selected physical capacities for one compatible adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompatibleLimits {
    http: HttpLimits,
    framing: FramingLimits,
    protocol: ProtocolLimits,
    max_fixed_headers: usize,
    max_fixed_header_value_bytes: usize,
}

impl CompatibleLimits {
    /// Production-wide capacities with no compatible-only hidden narrowing.
    pub const PRODUCTION: Self = Self {
        http: HttpLimits::PRODUCTION,
        framing: FramingLimits::PRODUCTION,
        protocol: ProtocolLimits::PRODUCTION,
        max_fixed_headers: HttpLimits::PRODUCTION.max_headers() - MANDATORY_REQUEST_HEADERS,
        max_fixed_header_value_bytes: HttpLimits::PRODUCTION.max_header_bytes(),
    };

    /// Creates an internally consistent set of compatible-adapter capacities.
    ///
    /// # Errors
    ///
    /// Rejects capacities that cannot fit the adapter's mandatory headers or that allow the HTTP
    /// transport to admit chunks, frames, or output larger than downstream consumers accept.
    pub fn new(
        http: HttpLimits,
        framing: FramingLimits,
        protocol: ProtocolLimits,
        max_fixed_headers: usize,
        max_fixed_header_value_bytes: usize,
    ) -> Result<Self, ProviderCoreError> {
        if http.max_headers() < MANDATORY_REQUEST_HEADERS
            || max_fixed_headers
                > http.max_headers().saturating_sub(MANDATORY_REQUEST_HEADERS)
            || max_fixed_header_value_bytes == 0
            || max_fixed_header_value_bytes > http.max_header_bytes()
            || http.max_chunk_bytes() > framing.max_buffer_bytes()
            || protocol.max_event_bytes() > framing.max_frame_bytes()
            || protocol.max_output_bytes() > http.max_response_body_bytes()
        {
            return Err(error::configuration(
                "compatible capacities are inconsistent across HTTP, framing, protocol, or fixed headers",
            ));
        }
        Ok(Self {
            http,
            framing,
            protocol,
            max_fixed_headers,
            max_fixed_header_value_bytes,
        })
    }

    /// Returns HTTP request, response, header, and chunk capacities.
    #[must_use]
    pub const fn http(self) -> HttpLimits {
        self.http
    }

    /// Returns incremental framing capacities.
    #[must_use]
    pub const fn framing(self) -> FramingLimits {
        self.framing
    }

    /// Returns normalized protocol capacities.
    #[must_use]
    pub const fn protocol(self) -> ProtocolLimits {
        self.protocol
    }

    /// Returns the maximum caller-supplied fixed-header count.
    #[must_use]
    pub const fn max_fixed_headers(self) -> usize {
        self.max_fixed_headers
    }

    /// Returns the maximum bytes in one caller-supplied fixed-header value.
    #[must_use]
    pub const fn max_fixed_header_value_bytes(self) -> usize {
        self.max_fixed_header_value_bytes
    }
}

impl Default for CompatibleLimits {
    fn default() -> Self {
        Self::PRODUCTION
    }
}

/// Exact compatible endpoint and transport policy.
#[derive(Clone)]
pub struct CompatibleConfig {
    endpoint: Endpoint,
    catalog_endpoint: Option<Endpoint>,
    hosted_service: Option<peritus_provider_core::hosted::HostedService>,
    auth: CompatibleAuth,
    fixed_headers: Vec<CompatibleHeader>,
    response_headers: CompatibleResponseHeaders,
    retry_statuses: CompatibleRetryStatuses,
    retry_policy: RetryPolicy,
    finite_retries: bool,
    limits: CompatibleLimits,
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
        let catalog_endpoint = derive_compatible_catalog_endpoint(&endpoint)?;
        Ok(Self {
            endpoint,
            catalog_endpoint,
            hosted_service: None,
            auth,
            fixed_headers: Vec::new(),
            response_headers: CompatibleResponseHeaders::none(),
            retry_statuses: CompatibleRetryStatuses::none(),
            retry_policy: RetryPolicy::without_deadline(
                3,
                [Duration::from_millis(100), Duration::from_secs(2), Duration::from_secs(2)],
                64 * 1024 * 1024,
            )?,
            finite_retries: false,
            limits: CompatibleLimits::PRODUCTION,
        })
    }

    /// Binds an explicit catalog endpoint for a direct compatible route.
    ///
    /// # Errors
    /// Rejects a different origin or replacement of a reviewed hosted-service catalog.
    pub fn with_catalog_endpoint(
        mut self,
        catalog_endpoint: Endpoint,
    ) -> Result<Self, ProviderCoreError> {
        if self.hosted_service.is_some() || !self.endpoint.same_origin(&catalog_endpoint) {
            return Err(error::configuration(
                "direct compatible catalog endpoint must share the inference origin",
            ));
        }
        self.catalog_endpoint = Some(catalog_endpoint);
        Ok(self)
    }

    /// Binds a reviewed hosted service's request and stream extensions to its exact endpoint.
    ///
    /// # Errors
    /// Rejects URLs outside this service's documented compatible operations.
    pub fn with_hosted_service(
        mut self,
        service: peritus_provider_core::hosted::HostedService,
    ) -> Result<Self, ProviderCoreError> {
        if ![
            peritus_model_protocol::WireDialect::CompatibleChatCompletions,
            peritus_model_protocol::WireDialect::CompatibleResponses,
        ]
        .into_iter()
        .any(|dialect| {
            service.route(dialect).is_ok_and(|route| route.endpoint == self.endpoint.as_str())
        }) {
            return Err(error::configuration(
                "hosted service does not own this compatible endpoint",
            ));
        }
        self.catalog_endpoint = Some(Endpoint::new(service.models_endpoint().to_owned())?);
        self.hosted_service = Some(service);
        Ok(self)
    }

    pub(crate) const fn hosted_service(
        &self,
    ) -> Option<peritus_provider_core::hosted::HostedService> {
        self.hosted_service
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
        validate_fixed_headers(&headers, self.limits)?;
        self.fixed_headers = headers;
        Ok(self)
    }

    /// Explicitly enables finite in-adapter retries with the supplied request budget.
    #[must_use]
    pub const fn with_retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self.finite_retries = true;
        self
    }

    /// Selects the compatible adapter's physical capacities.
    ///
    /// # Errors
    ///
    /// Rejects existing fixed headers that do not fit the selected capacities.
    pub fn with_limits(mut self, limits: CompatibleLimits) -> Result<Self, ProviderCoreError> {
        validate_fixed_headers(&self.fixed_headers, limits)?;
        self.limits = limits;
        Ok(self)
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

    /// Returns the exact reviewed catalog endpoint when discovery is configured.
    #[must_use]
    pub const fn catalog_endpoint(&self) -> Option<&Endpoint> {
        self.catalog_endpoint.as_ref()
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

    /// Returns the legacy finite retry bounds.
    ///
    /// These bounds are active only after [`Self::with_retry_policy`] explicitly enables adapter
    /// retries. New durable callers should inspect [`Self::finite_retry_policy`].
    #[must_use]
    pub const fn retry_policy(&self) -> RetryPolicy {
        self.retry_policy
    }

    /// Returns the explicitly enabled finite adapter retry policy, if any.
    #[must_use]
    pub const fn finite_retry_policy(&self) -> Option<RetryPolicy> {
        if self.finite_retries { Some(self.retry_policy) } else { None }
    }

    /// Returns all deliberately selected physical capacities.
    #[must_use]
    pub const fn limits(&self) -> CompatibleLimits {
        self.limits
    }

    /// Returns HTTP request, response, and header limits.
    #[must_use]
    pub const fn http_limits(&self) -> HttpLimits {
        self.limits.http()
    }

    /// Returns SSE framing limits.
    #[must_use]
    pub const fn framing_limits(&self) -> FramingLimits {
        self.limits.framing()
    }

    /// Returns provider-neutral event and output limits.
    #[must_use]
    pub const fn protocol_limits(&self) -> ProtocolLimits {
        self.limits.protocol()
    }
}

impl fmt::Debug for CompatibleConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CompatibleConfig")
            .field("endpoint", &self.endpoint)
            .field("catalog_endpoint", &self.catalog_endpoint)
            .field("hosted_service", &self.hosted_service)
            .field("auth", &self.auth)
            .field("fixed_headers", &self.fixed_headers)
            .field("response_headers", &self.response_headers)
            .field("retry_statuses", &self.retry_statuses)
            .field("retry_policy", &self.retry_policy)
            .field("finite_retries", &self.finite_retries)
            .field("limits", &self.limits)
            .finish()
    }
}

fn validate_fixed_headers(
    headers: &[CompatibleHeader],
    limits: CompatibleLimits,
) -> Result<(), ProviderCoreError> {
    let mut names = BTreeSet::new();
    if headers.len() > limits.max_fixed_headers()
        || headers
            .iter()
            .any(|header| header.value.len() > limits.max_fixed_header_value_bytes())
        || headers.iter().any(|header| !names.insert(header.name.as_str()))
    {
        return Err(error::configuration(
            "fixed compatible headers are duplicated or exceed selected capacities",
        ));
    }
    Ok(())
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
