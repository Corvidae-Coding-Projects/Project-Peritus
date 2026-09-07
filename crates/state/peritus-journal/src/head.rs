//! Aggregate compare-and-append heads.

use crate::{AggregateKey, CommittedRecord, JournalError, JournalErrorKind};
use peritus_types::{EventId, EventSequence, Sha256Digest};

/// Exact final event observed for one aggregate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AggregateHead {
    key: AggregateKey,
    sequence: EventSequence,
    event_id: EventId,
    event_hash: Sha256Digest,
}

impl AggregateHead {
    pub(crate) fn checked_successor(
        previous: Option<Self>,
        record: &CommittedRecord,
    ) -> Result<Self, JournalError> {
        let valid = previous.map_or_else(
            || {
                record.sequence().get() == 1
                    && record.previous_event_id().is_none()
                    && record.previous_event_hash() == Sha256Digest::new([0; 32])
            },
            |head| {
                head.key() == record.aggregate()
                    && head.sequence().get().checked_add(1) == Some(record.sequence().get())
                    && record.previous_event_id() == Some(head.event_id())
                    && record.previous_event_hash() == head.event_hash()
            },
        );
        if !valid {
            return Err(JournalError::new(
                JournalErrorKind::CorruptJournal,
                "validate aggregate chain",
                "aggregate sequence or hash predecessor is broken",
            ));
        }
        Ok(Self::new(record.aggregate(), record.sequence(), record.event_id(), record.event_hash()))
    }

    pub(crate) const fn new(
        key: AggregateKey,
        sequence: EventSequence,
        event_id: EventId,
        event_hash: Sha256Digest,
    ) -> Self {
        Self { key, sequence, event_id, event_hash }
    }

    /// Returns the aggregate key.
    #[must_use]
    pub const fn key(self) -> AggregateKey {
        self.key
    }

    /// Returns the one-based aggregate event sequence.
    #[must_use]
    pub const fn sequence(self) -> EventSequence {
        self.sequence
    }

    /// Returns the final event identity.
    #[must_use]
    pub const fn event_id(self) -> EventId {
        self.event_id
    }

    /// Returns the final event hash.
    #[must_use]
    pub const fn event_hash(self) -> Sha256Digest {
        self.event_hash
    }
}
