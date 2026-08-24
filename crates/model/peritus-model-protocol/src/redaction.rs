//! Bounded allowlisted diagnostics that cannot contain provider payloads.

use core::fmt;

use crate::{ProtocolError, ProtocolErrorKind};

/// Redaction-safe stable diagnostic code and numeric observations.
#[derive(Clone, Eq, PartialEq)]
pub struct RedactedDiagnostic {
    code: String,
    request_bytes: Option<u64>,
    response_bytes: Option<u64>,
    elapsed_millis: Option<u64>,
}

impl RedactedDiagnostic {
    /// Creates a bounded machine-readable code with optional allowlisted counters.
    ///
    /// # Errors
    ///
    /// Rejects empty/non-ASCII/oversized codes and characters outside `[a-z0-9_.-]`.
    pub fn new(
        code: String,
        request_bytes: Option<u64>,
        response_bytes: Option<u64>,
        elapsed_millis: Option<u64>,
    ) -> Result<Self, ProtocolError> {
        if code.is_empty()
            || code.len() > 128
            || !code.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'_' | b'.' | b'-')
            })
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidIdentity,
                "diagnostic.code",
                "diagnostic code is malformed or exceeds its byte bound",
            ));
        }
        Ok(Self { code, request_bytes, response_bytes, elapsed_millis })
    }

    /// Borrows the stable code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }
    /// Returns observed request bytes.
    #[must_use]
    pub const fn request_bytes(&self) -> Option<u64> {
        self.request_bytes
    }
    /// Returns observed response bytes.
    #[must_use]
    pub const fn response_bytes(&self) -> Option<u64> {
        self.response_bytes
    }
    /// Returns observed elapsed milliseconds.
    #[must_use]
    pub const fn elapsed_millis(&self) -> Option<u64> {
        self.elapsed_millis
    }
}

impl fmt::Debug for RedactedDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RedactedDiagnostic")
            .field("code", &self.code)
            .field("request_bytes", &self.request_bytes)
            .field("response_bytes", &self.response_bytes)
            .field("elapsed_millis", &self.elapsed_millis)
            .finish()
    }
}
