//! Aggregate compare-and-append heads.

use crate::AggregateKey;
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
