//! Transactional outbox inputs and durable delivery state.

use crate::{JournalError, JournalErrorKind, OutboxId};

/// Maximum destination bytes.
pub const MAX_DESTINATION_BYTES: usize = 512;
/// Maximum opaque transport bytes in one outbox row.
pub const MAX_OUTBOX_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;

/// Checked outbox message planned with its producing events.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboxDraft {
    id: OutboxId,
    destination: String,
    payload: Vec<u8>,
    max_attempts: u16,
}

/// Exact claimed outbox row to acknowledge in the same transaction as an aggregate append.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OutboxAcknowledgement {
    id: OutboxId,
    fence: u64,
}

impl OutboxAcknowledgement {
    /// Creates an acknowledgement bound to a positive claim fence.
    ///
    /// # Errors
    ///
    /// Rejects the reserved zero fence.
    pub const fn new(id: OutboxId, fence: u64) -> Result<Self, JournalError> {
        if fence == 0 {
            return Err(JournalError::new(
                JournalErrorKind::InvalidInput,
                "validate outbox acknowledgement",
                "outbox fence must be positive",
            ));
        }
        Ok(Self { id, fence })
    }

    /// Returns the exact outbox identity.
    #[must_use]
    pub const fn id(self) -> OutboxId {
        self.id
    }

    /// Returns the exact claim fence.
    #[must_use]
    pub const fn fence(self) -> u64 {
        self.fence
    }
}

impl OutboxDraft {
    /// Validates one bounded destination and transport payload.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, control-character destinations, oversized payloads, and zero
    /// attempt limits.
    pub fn new(
        id: OutboxId,
        destination: String,
        payload: Vec<u8>,
        max_attempts: u16,
    ) -> Result<Self, JournalError> {
        let valid_destination = !destination.is_empty()
            && destination.len() <= MAX_DESTINATION_BYTES
            && destination.bytes().all(|byte| byte.is_ascii_graphic());
        if !valid_destination || payload.len() > MAX_OUTBOX_PAYLOAD_BYTES || max_attempts == 0 {
            return Err(JournalError::new(
                JournalErrorKind::InvalidInput,
                "validate outbox entry",
                "invalid destination, payload bound, or attempt limit",
            ));
        }
        Ok(Self { id, destination, payload, max_attempts })
    }

    /// Returns the message identity.
    #[must_use]
    pub const fn id(&self) -> OutboxId {
        self.id
    }

    /// Returns the exact destination.
    #[must_use]
    pub fn destination(&self) -> &str {
        &self.destination
    }

    /// Borrows exact opaque transport bytes.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Returns the bounded attempt limit.
    #[must_use]
    pub const fn max_attempts(&self) -> u16 {
        self.max_attempts
    }
}

/// Durable outbox lifecycle state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OutboxState {
    /// Available to claim.
    Pending,
    /// Claimed until a durable lease deadline under a fence token.
    Claimed,
    /// Idempotently acknowledged.
    Acknowledged,
    /// The configured attempt bound was exhausted.
    Exhausted,
}

/// Checked durable outbox observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboxMessage {
    pub(crate) id: OutboxId,
    pub(crate) producing_position: u64,
    pub(crate) destination: String,
    pub(crate) payload: Vec<u8>,
    pub(crate) attempts: u16,
    pub(crate) max_attempts: u16,
    pub(crate) state: OutboxState,
    pub(crate) fence: Option<u64>,
    pub(crate) lease_until: Option<u64>,
}

impl OutboxMessage {
    /// Returns the message identity.
    #[must_use]
    pub const fn id(&self) -> OutboxId {
        self.id
    }

    /// Returns the producing event position.
    #[must_use]
    pub const fn producing_position(&self) -> u64 {
        self.producing_position
    }

    /// Returns the transport destination.
    #[must_use]
    pub fn destination(&self) -> &str {
        &self.destination
    }

    /// Borrows exact transport bytes.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Returns attempts already claimed.
    #[must_use]
    pub const fn attempts(&self) -> u16 {
        self.attempts
    }

    /// Returns the configured attempt bound.
    #[must_use]
    pub const fn max_attempts(&self) -> u16 {
        self.max_attempts
    }

    /// Returns the durable delivery state.
    #[must_use]
    pub const fn state(&self) -> OutboxState {
        self.state
    }

    /// Returns the current claim fence, if claimed.
    #[must_use]
    pub const fn fence(&self) -> Option<u64> {
        self.fence
    }

    /// Returns the current lease deadline, if claimed.
    #[must_use]
    pub const fn lease_until(&self) -> Option<u64> {
        self.lease_until
    }
}
