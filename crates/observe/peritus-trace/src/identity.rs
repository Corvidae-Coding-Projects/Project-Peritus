//! OpenTelemetry-compatible nonzero trace and span identities.

use crate::{TraceError, TraceErrorKind};

/// Nonzero 16-byte distributed trace identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TraceId([u8; 16]);

impl TraceId {
    /// Exact binary representation length.
    pub const LENGTH: usize = 16;

    /// Creates a trace identity, rejecting the OpenTelemetry invalid all-zero value.
    ///
    /// # Errors
    ///
    /// Returns an invalid-identity error for the all-zero representation.
    pub const fn new(bytes: [u8; Self::LENGTH]) -> Result<Self, TraceError> {
        if nonzero_16(bytes) {
            Ok(Self(bytes))
        } else {
            Err(TraceError::static_error(
                TraceErrorKind::InvalidIdentity,
                "validate trace identity",
                "all-zero trace identity is reserved",
            ))
        }
    }

    /// Borrows the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; Self::LENGTH] {
        &self.0
    }

    /// Returns the exact identity bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; Self::LENGTH] {
        self.0
    }
}

/// Nonzero 8-byte span identity compatible with OpenTelemetry exporters.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SpanId([u8; 8]);

impl SpanId {
    /// Exact binary representation length.
    pub const LENGTH: usize = 8;

    /// Creates a span identity, rejecting the OpenTelemetry invalid all-zero value.
    ///
    /// # Errors
    ///
    /// Returns an invalid-identity error for the all-zero representation.
    pub const fn new(bytes: [u8; Self::LENGTH]) -> Result<Self, TraceError> {
        if nonzero_8(bytes) {
            Ok(Self(bytes))
        } else {
            Err(TraceError::static_error(
                TraceErrorKind::InvalidIdentity,
                "validate span identity",
                "all-zero span identity is reserved",
            ))
        }
    }

    /// Borrows the exact identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; Self::LENGTH] {
        &self.0
    }

    /// Returns the exact identity bytes.
    #[must_use]
    pub const fn into_bytes(self) -> [u8; Self::LENGTH] {
        self.0
    }
}

const fn nonzero_16(bytes: [u8; 16]) -> bool {
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != 0 {
            return true;
        }
        index += 1;
    }
    false
}

const fn nonzero_8(bytes: [u8; 8]) -> bool {
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != 0 {
            return true;
        }
        index += 1;
    }
    false
}
