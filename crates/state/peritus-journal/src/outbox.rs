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
    delivery_policy: OutboxDeliveryPolicy,
}

/// Durable outbox delivery policy, independent of the effect's semantic retry policy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OutboxDeliveryPolicy {
    /// Delivery stops after an explicit positive number of claims.
    Bounded {
        /// Maximum number of claim fences that may be issued.
        max_attempts: u16,
    },
    /// The same durable row remains reclaimable until it is acknowledged.
    Persistent,
}

impl OutboxDeliveryPolicy {
    pub(crate) const fn stored_max_attempts(self) -> u16 {
        match self {
            Self::Bounded { max_attempts } => max_attempts,
            Self::Persistent => u16::MAX,
        }
    }

    pub(crate) const fn is_persistent(self) -> bool {
        matches!(self, Self::Persistent)
    }
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
        Ok(Self {
            id,
            destination,
            payload,
            delivery_policy: OutboxDeliveryPolicy::Bounded { max_attempts },
        })
    }

    /// Validates one directive that remains reclaimable until exact acknowledgement.
    ///
    /// # Errors
    ///
    /// Rejects empty, oversized, control-character destinations and oversized payloads.
    pub fn persistent(
        id: OutboxId,
        destination: String,
        payload: Vec<u8>,
    ) -> Result<Self, JournalError> {
        let valid_destination = !destination.is_empty()
            && destination.len() <= MAX_DESTINATION_BYTES
            && destination.bytes().all(|byte| byte.is_ascii_graphic());
        if !valid_destination || payload.len() > MAX_OUTBOX_PAYLOAD_BYTES {
            return Err(JournalError::new(
                JournalErrorKind::InvalidInput,
                "validate persistent outbox entry",
                "invalid destination or payload bound",
            ));
        }
        Ok(Self {
            id,
            destination,
            payload,
            delivery_policy: OutboxDeliveryPolicy::Persistent,
        })
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
        self.delivery_policy.stored_max_attempts()
    }

    /// Returns the delivery policy, which is separate from effect-level retry budgets.
    #[must_use]
    pub const fn delivery_policy(&self) -> OutboxDeliveryPolicy {
        self.delivery_policy
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

/// Time-aware availability of one exact durable outbox row.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OutboxDeliveryStatus {
    /// The row is immediately available for its first or next claim.
    Pending,
    /// Another live claim fence owns the row until the stated tick.
    Waiting {
        /// Current positive claim fence.
        fence: u64,
        /// Inclusive tick at which the row becomes reclaimable.
        lease_until: u64,
    },
    /// The prior claim lease elapsed and the same row may be reclaimed under a new fence.
    Reclaimable {
        /// Prior positive claim fence that must no longer settle the row.
        prior_fence: u64,
        /// Elapsed lease boundary.
        lease_until: u64,
    },
    /// The row was exactly acknowledged.
    Acknowledged,
    /// An accepted bounded-delivery row reached its explicit limit.
    Exhausted,
}

/// Checked durable outbox observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutboxMessage {
    pub(crate) id: OutboxId,
    pub(crate) producing_position: u64,
    pub(crate) destination: String,
    pub(crate) payload: Vec<u8>,
    pub(crate) attempts: u64,
    pub(crate) max_attempts: u16,
    pub(crate) persistent: bool,
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
    pub const fn attempts(&self) -> u64 {
        self.attempts
    }

    /// Returns the configured attempt bound.
    #[must_use]
    pub const fn max_attempts(&self) -> u16 {
        self.max_attempts
    }

    /// Returns the delivery policy, independent of any semantic attempt budget in the payload.
    #[must_use]
    pub const fn delivery_policy(&self) -> OutboxDeliveryPolicy {
        if self.persistent {
            OutboxDeliveryPolicy::Persistent
        } else {
            OutboxDeliveryPolicy::Bounded { max_attempts: self.max_attempts }
        }
    }

    /// Returns whether a bounded row has reached its claim limit.
    #[must_use]
    pub const fn delivery_attempts_exhausted(&self) -> bool {
        !self.persistent && self.attempts >= self.max_attempts as u64
    }

    /// Classifies whether the exact row must wait or may be reclaimed at a positive tick.
    ///
    /// # Errors
    ///
    /// Rejects tick zero or internally inconsistent claimed-row metadata.
    pub fn delivery_status(
        &self,
        observed_at: u64,
    ) -> Result<OutboxDeliveryStatus, JournalError> {
        if observed_at == 0 {
            return Err(JournalError::new(
                JournalErrorKind::InvalidInput,
                "classify outbox delivery",
                "outbox observation tick must be positive",
            ));
        }
        match self.state {
            OutboxState::Pending => Ok(OutboxDeliveryStatus::Pending),
            OutboxState::Claimed => {
                let fence = self.fence.ok_or_else(|| {
                    JournalError::new(
                        JournalErrorKind::CorruptJournal,
                        "classify outbox delivery",
                        "claimed outbox row has no fence",
                    )
                })?;
                let lease_until = self.lease_until.ok_or_else(|| {
                    JournalError::new(
                        JournalErrorKind::CorruptJournal,
                        "classify outbox delivery",
                        "claimed outbox row has no lease boundary",
                    )
                })?;
                if observed_at >= lease_until {
                    Ok(OutboxDeliveryStatus::Reclaimable {
                        prior_fence: fence,
                        lease_until,
                    })
                } else {
                    Ok(OutboxDeliveryStatus::Waiting { fence, lease_until })
                }
            }
            OutboxState::Acknowledged => Ok(OutboxDeliveryStatus::Acknowledged),
            OutboxState::Exhausted if !self.persistent => Ok(OutboxDeliveryStatus::Exhausted),
            OutboxState::Exhausted => Err(JournalError::new(
                JournalErrorKind::CorruptJournal,
                "classify outbox delivery",
                "persistent outbox row is marked exhausted",
            )),
        }
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
