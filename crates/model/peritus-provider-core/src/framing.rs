//! Incremental bounded Server-Sent Events and newline-delimited JSON framing.

use crate::{ProviderCoreError, ProviderCoreErrorKind};

mod ndjson;
mod sse;

pub use ndjson::{NdjsonFrame, NdjsonParser};
pub use sse::{SseComment, SseFrame, SseItem, SseParser};

const PRODUCTION_MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const PRODUCTION_MAX_BUFFER_BYTES: usize = 16 * 1024 * 1024;

/// Resource ceilings for incremental wire framing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FramingLimits {
    max_frame_bytes: usize,
    max_buffer_bytes: usize,
}

impl FramingLimits {
    /// Production-wide framing ceilings.
    pub const PRODUCTION: Self = Self {
        max_frame_bytes: PRODUCTION_MAX_FRAME_BYTES,
        max_buffer_bytes: PRODUCTION_MAX_BUFFER_BYTES,
    };

    /// Creates framing limits.
    ///
    /// # Errors
    ///
    /// Rejects zero, widened, or internally inconsistent limits.
    pub const fn new(
        max_frame_bytes: usize,
        max_buffer_bytes: usize,
    ) -> Result<Self, ProviderCoreError> {
        if max_frame_bytes == 0
            || max_buffer_bytes == 0
            || max_frame_bytes > PRODUCTION_MAX_FRAME_BYTES
            || max_buffer_bytes > PRODUCTION_MAX_BUFFER_BYTES
            || max_frame_bytes > max_buffer_bytes
        {
            return Err(framing_error(
                ProviderCoreErrorKind::LimitExceeded,
                "framing limits must be nonzero, consistent, and within production ceilings",
            ));
        }
        Ok(Self { max_frame_bytes, max_buffer_bytes })
    }

    /// Maximum bytes in one logical frame.
    #[must_use]
    pub const fn max_frame_bytes(self) -> usize {
        self.max_frame_bytes
    }

    /// Maximum unprocessed bytes retained between pushes.
    #[must_use]
    pub const fn max_buffer_bytes(self) -> usize {
        self.max_buffer_bytes
    }
}

impl Default for FramingLimits {
    fn default() -> Self {
        Self::PRODUCTION
    }
}

pub fn strip_carriage_return(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

pub const fn malformed(detail: &'static str) -> ProviderCoreError {
    framing_error(ProviderCoreErrorKind::MalformedStream, detail)
}

pub const fn limit(detail: &'static str) -> ProviderCoreError {
    framing_error(ProviderCoreErrorKind::LimitExceeded, detail)
}

const fn framing_error(kind: ProviderCoreErrorKind, detail: &'static str) -> ProviderCoreError {
    ProviderCoreError::new(kind, "framing", detail)
}
