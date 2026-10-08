//! Sensitive request and provider identities with explicit redacted diagnostics.

use core::fmt;

use super::CheckedIdentity;
use crate::ProtocolError;

/// Checked sensitive caller request identity.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RequestId(CheckedIdentity);

impl RequestId {
    /// Creates a checked sensitive caller request identity.
    ///
    /// # Errors
    ///
    /// Rejects empty or control-containing input. Enclosing transports own physical byte limits.
    pub fn new(value: String) -> Result<Self, ProtocolError> {
        CheckedIdentity::new(value, "request_id").map(Self)
    }

    /// Borrows the checked value for wire projection.
    #[must_use]
    pub fn expose_for_wire(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for RequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RequestId([redacted])")
    }
}

/// Checked sensitive provider response identity.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ResponseId(CheckedIdentity);

impl ResponseId {
    /// Creates a checked sensitive provider response identity.
    ///
    /// # Errors
    ///
    /// Rejects empty or control-containing input. Enclosing transports own physical byte limits.
    pub fn new(value: String) -> Result<Self, ProtocolError> {
        CheckedIdentity::new(value, "response_id").map(Self)
    }

    /// Borrows the checked value for wire projection.
    #[must_use]
    pub fn expose_for_wire(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for ResponseId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ResponseId([redacted])")
    }
}

/// Checked sensitive provider item identity.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ItemId(CheckedIdentity);

impl ItemId {
    /// Creates a checked sensitive provider item identity.
    ///
    /// # Errors
    ///
    /// Rejects empty or control-containing input. Enclosing transports own physical byte limits.
    pub fn new(value: String) -> Result<Self, ProtocolError> {
        CheckedIdentity::new(value, "item_id").map(Self)
    }

    /// Borrows the checked value for wire projection.
    #[must_use]
    pub fn expose_for_wire(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for ItemId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ItemId([redacted])")
    }
}

/// Checked sensitive tool-call identity.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ToolCallId(CheckedIdentity);

impl ToolCallId {
    /// Creates a checked sensitive tool-call identity.
    ///
    /// # Errors
    ///
    /// Rejects empty or control-containing input. Enclosing transports own physical byte limits.
    pub fn new(value: String) -> Result<Self, ProtocolError> {
        CheckedIdentity::new(value, "tool_call_id").map(Self)
    }

    /// Borrows the checked value for wire projection.
    #[must_use]
    pub fn expose_for_wire(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for ToolCallId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ToolCallId([redacted])")
    }
}

/// Checked sensitive provider event identity.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EventId(CheckedIdentity);

impl EventId {
    /// Creates a checked sensitive provider event identity.
    ///
    /// # Errors
    ///
    /// Rejects empty or control-containing input. Enclosing transports own physical byte limits.
    pub fn new(value: String) -> Result<Self, ProtocolError> {
        CheckedIdentity::new(value, "event_id").map(Self)
    }

    /// Borrows the checked value for wire projection.
    #[must_use]
    pub fn expose_for_wire(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for EventId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EventId([redacted])")
    }
}

/// Checked sensitive provider cache identity.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CacheKey(CheckedIdentity);

impl CacheKey {
    /// Creates a checked sensitive provider cache identity.
    ///
    /// # Errors
    ///
    /// Rejects empty or control-containing input. Provider adapters validate mapped cache fields.
    pub fn new(value: String) -> Result<Self, ProtocolError> {
        CheckedIdentity::new(value, "cache_key").map(Self)
    }

    /// Borrows the checked value for wire projection.
    #[must_use]
    pub fn expose_for_wire(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for CacheKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CacheKey([redacted])")
    }
}

/// Checked sensitive idempotency identity.
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IdempotencyKey(CheckedIdentity);

impl IdempotencyKey {
    /// Creates a checked sensitive idempotency identity.
    ///
    /// # Errors
    ///
    /// Rejects empty or control-containing input. Provider adapters validate mapped wire fields.
    pub fn new(value: String) -> Result<Self, ProtocolError> {
        CheckedIdentity::new(value, "idempotency_key").map(Self)
    }

    /// Borrows the checked value for wire projection.
    #[must_use]
    pub fn expose_for_wire(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for IdempotencyKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IdempotencyKey([redacted])")
    }
}
