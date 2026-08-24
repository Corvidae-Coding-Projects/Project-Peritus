//! Bounded values whose formatting is always redacted.

use core::fmt;

use crate::{ProviderCoreError, ProviderCoreErrorKind};

const MAX_REDACTED_VALUE_BYTES: usize = 1_024;

/// A checked value that never reveals its contents through `Debug` or `Display`.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RedactedValue(String);

impl RedactedValue {
    /// Creates a bounded redacted value.
    ///
    /// # Errors
    ///
    /// Rejects empty, control-containing, or oversized input.
    pub fn new(value: String) -> Result<Self, ProviderCoreError> {
        if value.is_empty()
            || value.len() > MAX_REDACTED_VALUE_BYTES
            || value.chars().any(char::is_control)
        {
            return Err(ProviderCoreError::new(
                ProviderCoreErrorKind::InvalidHttp,
                "redacted_value",
                "redacted value is empty, contains controls, or exceeds its byte bound",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows the checked value for an explicitly allowlisted use.
    #[must_use]
    pub fn expose_allowlisted(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RedactedValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RedactedValue([redacted])")
    }
}

impl fmt::Display for RedactedValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[redacted]")
    }
}
