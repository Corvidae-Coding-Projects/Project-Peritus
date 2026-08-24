//! Explicit retry and response-header mappings; no provider convention is inferred.

use peritus_model_protocol::RateLimitDimension;
use peritus_provider_core::{HeaderName, ProviderCoreError};

use super::{reserved_header, secret_header_name};
use crate::error;

/// Explicit status classes documented as temporary non-accepting rejections.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompatibleRetryStatuses {
    rate_limited: bool,
    server: bool,
}

impl CompatibleRetryStatuses {
    /// Creates the minimum-safe mapping with no retryable HTTP status assumptions.
    #[must_use]
    pub const fn none() -> Self {
        Self { rate_limited: false, server: false }
    }

    /// Declares HTTP 429 as a temporary, explicitly rejected request.
    #[must_use]
    pub const fn with_rate_limited(mut self) -> Self {
        self.rate_limited = true;
        self
    }

    /// Declares HTTP 5xx as temporary, explicitly rejected requests.
    #[must_use]
    pub const fn with_server_errors(mut self) -> Self {
        self.server = true;
        self
    }

    /// Returns whether 429 is documented as temporary.
    #[must_use]
    pub const fn rate_limited(self) -> bool {
        self.rate_limited
    }

    /// Returns whether 5xx statuses are documented as temporary.
    #[must_use]
    pub const fn server_errors(self) -> bool {
        self.server
    }
}

/// Explicit response-header mappings for one compatible endpoint.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompatibleResponseHeaders {
    request_id: Option<HeaderName>,
    rate_limit: Option<CompatibleRateHeaders>,
}

impl CompatibleResponseHeaders {
    /// Creates an empty mapping. No provider-specific response header is inferred.
    #[must_use]
    pub const fn none() -> Self {
        Self { request_id: None, rate_limit: None }
    }

    /// Maps one explicitly documented nonsensitive provider request-ID header.
    ///
    /// # Errors
    ///
    /// Rejects a secret-bearing or transport-controlled header name.
    pub fn with_request_id(mut self, name: HeaderName) -> Result<Self, ProviderCoreError> {
        if secret_header_name(name.as_str()) || reserved_header(name.as_str()) {
            return Err(error::configuration(
                "compatible request-ID mapping uses an unsafe response header",
            ));
        }
        self.request_id = Some(name);
        Ok(self)
    }

    /// Maps one explicitly documented rate-limit header group.
    #[must_use]
    pub fn with_rate_limit(mut self, mapping: CompatibleRateHeaders) -> Self {
        self.rate_limit = Some(mapping);
        self
    }

    /// Returns the request-ID response header when explicitly mapped.
    #[must_use]
    pub const fn request_id(&self) -> Option<&HeaderName> {
        self.request_id.as_ref()
    }

    /// Returns the rate-limit header mapping when explicitly mapped.
    #[must_use]
    pub const fn rate_limit(&self) -> Option<&CompatibleRateHeaders> {
        self.rate_limit.as_ref()
    }
}

/// Exact limit/remaining/reset header names and their provider-neutral dimension.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibleRateHeaders {
    limit: HeaderName,
    remaining: HeaderName,
    reset: HeaderName,
    dimension: RateLimitDimension,
    reset_unit: CompatibleResetUnit,
}

/// Exact unit used by one numeric compatible rate-limit reset header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatibleResetUnit {
    /// Relative milliseconds.
    Milliseconds,
    /// Relative seconds.
    Seconds,
}

impl CompatibleRateHeaders {
    /// Creates one explicit bounded rate-limit mapping.
    ///
    /// # Errors
    ///
    /// Rejects duplicate, secret-bearing, or transport-controlled response headers.
    pub fn new(
        limit: HeaderName,
        remaining: HeaderName,
        reset: HeaderName,
        dimension: RateLimitDimension,
        reset_unit: CompatibleResetUnit,
    ) -> Result<Self, ProviderCoreError> {
        let names = [limit.as_str(), remaining.as_str(), reset.as_str()];
        if names[0] == names[1]
            || names[0] == names[2]
            || names[1] == names[2]
            || names.iter().any(|name| secret_header_name(name) || reserved_header(name))
        {
            return Err(error::configuration(
                "compatible rate-limit mapping uses duplicate or unsafe response headers",
            ));
        }
        Ok(Self { limit, remaining, reset, dimension, reset_unit })
    }

    /// Returns the total-limit header name.
    #[must_use]
    pub const fn limit(&self) -> &HeaderName {
        &self.limit
    }

    /// Returns the remaining-limit header name.
    #[must_use]
    pub const fn remaining(&self) -> &HeaderName {
        &self.remaining
    }

    /// Returns the reset-delay header name.
    #[must_use]
    pub const fn reset(&self) -> &HeaderName {
        &self.reset
    }

    /// Returns the provider-neutral limited dimension.
    #[must_use]
    pub const fn dimension(&self) -> &RateLimitDimension {
        &self.dimension
    }

    /// Returns the exact numeric reset unit.
    #[must_use]
    pub const fn reset_unit(&self) -> CompatibleResetUnit {
        self.reset_unit
    }
}
