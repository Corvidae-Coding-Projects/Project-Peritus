//! Provider rate-limit and cache observations without authority semantics.

use crate::{CacheKey, ExtensionName, ProtocolError, ProtocolErrorKind};

/// Portable rate-limited resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RateLimitDimension {
    /// Requests in a provider window.
    Requests,
    /// Input tokens.
    InputTokens,
    /// Output tokens.
    OutputTokens,
    /// Combined tokens.
    TotalTokens,
    /// Generated/accepted images.
    Images,
    /// Provider daily requests.
    DailyRequests,
    /// Explicit provider-specific dimension.
    Provider(ExtensionName),
}

/// Provider reset representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResetTime {
    /// Duration from observation in milliseconds.
    AfterMillis(u64),
    /// Unix timestamp in milliseconds.
    UnixMillis(u64),
}

/// One bounded provider rate-limit window.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitWindow {
    dimension: RateLimitDimension,
    limit: Option<u64>,
    remaining: Option<u64>,
    reset: Option<ResetTime>,
}

impl RateLimitWindow {
    /// Creates a window and validates known remaining/limit values.
    ///
    /// # Errors
    ///
    /// Rejects remaining greater than the declared limit.
    pub fn new(
        dimension: RateLimitDimension,
        limit: Option<u64>,
        remaining: Option<u64>,
        reset: Option<ResetTime>,
    ) -> Result<Self, ProtocolError> {
        if matches!((limit, remaining), (Some(limit), Some(remaining)) if remaining > limit) {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidUsage,
                "rate_limit",
                "remaining quantity exceeds its declared limit",
            ));
        }
        Ok(Self { dimension, limit, remaining, reset })
    }

    /// Borrows the dimension.
    #[must_use]
    pub const fn dimension(&self) -> &RateLimitDimension {
        &self.dimension
    }
    /// Returns the optional limit.
    #[must_use]
    pub const fn limit(&self) -> Option<u64> {
        self.limit
    }
    /// Returns the optional remaining quantity.
    #[must_use]
    pub const fn remaining(&self) -> Option<u64> {
        self.remaining
    }
    /// Returns the optional reset.
    #[must_use]
    pub const fn reset(&self) -> Option<ResetTime> {
        self.reset
    }
}

/// A bounded set of windows from one response observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitObservation(Vec<RateLimitWindow>);

impl RateLimitObservation {
    /// Creates a nonempty observation with at most 64 dimensions.
    ///
    /// # Errors
    ///
    /// Rejects empty or oversized window sets.
    pub fn new(windows: Vec<RateLimitWindow>) -> Result<Self, ProtocolError> {
        if windows.is_empty() || windows.len() > 64 {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidUsage,
                "rate_limit",
                "rate-limit observation is empty or exceeds its window bound",
            ));
        }
        Ok(Self(windows))
    }

    /// Borrows windows in provider observation order.
    #[must_use]
    pub fn windows(&self) -> &[RateLimitWindow] {
        &self.0
    }
}

/// Provider cache outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheStatus {
    /// Provider reports a hit.
    Hit,
    /// Provider reports a miss.
    Miss,
    /// Provider created a cache entry.
    Created,
    /// Provider bypassed caching.
    Bypassed,
    /// Provider reported a cache result not covered by the portable taxonomy.
    Unknown,
}

/// One cache observation; it does not alter B1 accounting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheObservation {
    status: CacheStatus,
    key: Option<CacheKey>,
    input_tokens: Option<u64>,
    ttl_seconds: Option<u64>,
}

impl CacheObservation {
    /// Creates one normalized cache observation.
    #[must_use]
    pub const fn new(
        status: CacheStatus,
        key: Option<CacheKey>,
        input_tokens: Option<u64>,
        ttl_seconds: Option<u64>,
    ) -> Self {
        Self { status, key, input_tokens, ttl_seconds }
    }

    /// Returns the status.
    #[must_use]
    pub const fn status(&self) -> CacheStatus {
        self.status
    }
    /// Borrows the optional sensitive provider key.
    #[must_use]
    pub const fn key(&self) -> Option<&CacheKey> {
        self.key.as_ref()
    }
    /// Returns cache-attributed input tokens.
    #[must_use]
    pub const fn input_tokens(&self) -> Option<u64> {
        self.input_tokens
    }
    /// Returns observed TTL seconds.
    #[must_use]
    pub const fn ttl_seconds(&self) -> Option<u64> {
        self.ttl_seconds
    }
}
