//! HTTP resource ceilings.

use crate::{ProviderCoreError, ProviderCoreErrorKind};

use super::http_error;

const PRODUCTION_MAX_HEADERS: usize = 256;
const PRODUCTION_MAX_HEADER_BYTES: usize = 128 * 1024;
const PRODUCTION_MAX_REQUEST_BODY_BYTES: usize = 32 * 1024 * 1024;
const PRODUCTION_MAX_RESPONSE_BODY_BYTES: usize = 256 * 1024 * 1024;
const PRODUCTION_MAX_CHUNK_BYTES: usize = 1024 * 1024;

/// Resource ceilings enforced at the HTTP ownership boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_field_names,
    reason = "the max_ prefix distinguishes configured ceilings from observed counts"
)]
pub struct HttpLimits {
    max_headers: usize,
    max_header_bytes: usize,
    max_request_body_bytes: usize,
    max_response_body_bytes: usize,
    max_chunk_bytes: usize,
}

impl HttpLimits {
    /// Production-wide HTTP ceilings.
    pub const PRODUCTION: Self = Self {
        max_headers: PRODUCTION_MAX_HEADERS,
        max_header_bytes: PRODUCTION_MAX_HEADER_BYTES,
        max_request_body_bytes: PRODUCTION_MAX_REQUEST_BODY_BYTES,
        max_response_body_bytes: PRODUCTION_MAX_RESPONSE_BODY_BYTES,
        max_chunk_bytes: PRODUCTION_MAX_CHUNK_BYTES,
    };

    /// Creates nonzero ceilings no wider than the production limits.
    ///
    /// Values are ordered as header count, cumulative header bytes, request-body bytes,
    /// response-body bytes, and body-chunk bytes.
    ///
    /// # Errors
    ///
    /// Rejects a zero, widened, or internally inconsistent limit.
    pub fn new(values: [usize; 5]) -> Result<Self, ProviderCoreError> {
        let production = Self::PRODUCTION.as_array();
        if values.iter().zip(production).any(|(value, ceiling)| *value == 0 || *value > ceiling)
            || values[4] > values[3]
        {
            return Err(http_error(
                ProviderCoreErrorKind::LimitExceeded,
                "HTTP limits must be nonzero, internally consistent, and within production ceilings",
            ));
        }
        Ok(Self {
            max_headers: values[0],
            max_header_bytes: values[1],
            max_request_body_bytes: values[2],
            max_response_body_bytes: values[3],
            max_chunk_bytes: values[4],
        })
    }

    /// Returns limits in stable constructor order.
    #[must_use]
    pub const fn as_array(self) -> [usize; 5] {
        [
            self.max_headers,
            self.max_header_bytes,
            self.max_request_body_bytes,
            self.max_response_body_bytes,
            self.max_chunk_bytes,
        ]
    }

    /// Maximum header count.
    #[must_use]
    pub const fn max_headers(self) -> usize {
        self.max_headers
    }

    /// Maximum cumulative header-name and value bytes.
    #[must_use]
    pub const fn max_header_bytes(self) -> usize {
        self.max_header_bytes
    }

    /// Maximum request-body bytes.
    #[must_use]
    pub const fn max_request_body_bytes(self) -> usize {
        self.max_request_body_bytes
    }

    /// Maximum response-body bytes.
    #[must_use]
    pub const fn max_response_body_bytes(self) -> usize {
        self.max_response_body_bytes
    }

    /// Maximum bytes yielded by one body-stream pull.
    #[must_use]
    pub const fn max_chunk_bytes(self) -> usize {
        self.max_chunk_bytes
    }
}

impl Default for HttpLimits {
    fn default() -> Self {
        Self::PRODUCTION
    }
}
