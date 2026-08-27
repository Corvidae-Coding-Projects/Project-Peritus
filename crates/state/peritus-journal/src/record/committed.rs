//! Digest-checked immutable event observations.

use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};

use super::ExactFrame;
use crate::AggregateKey;

/// One checked immutable event read from committed storage.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommittedRecord {
    pub(crate) global_position: u64,
    pub(crate) aggregate: AggregateKey,
    pub(crate) sequence: EventSequence,
    pub(crate) event_id: EventId,
    pub(crate) previous_event_id: Option<EventId>,
    pub(crate) previous_event_hash: Sha256Digest,
    pub(crate) event_hash: Sha256Digest,
    pub(crate) command_id: CommandId,
    pub(crate) frame: ExactFrame,
    pub(crate) revision_digest: Sha256Digest,
    pub(crate) causal_parents: Vec<EventId>,
}

impl CommittedRecord {
    /// Returns the one-based store-wide position.
    #[must_use]
    pub const fn global_position(&self) -> u64 {
        self.global_position
    }

    /// Returns the aggregate key.
    #[must_use]
    pub const fn aggregate(&self) -> AggregateKey {
        self.aggregate
    }

    /// Returns the one-based aggregate sequence.
    #[must_use]
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }

    /// Returns the event identity.
    #[must_use]
    pub const fn event_id(&self) -> EventId {
        self.event_id
    }

    /// Returns the exact predecessor identity, or genesis.
    #[must_use]
    pub const fn previous_event_id(&self) -> Option<EventId> {
        self.previous_event_id
    }

    /// Returns the predecessor event hash, or the declared zero digest at genesis.
    #[must_use]
    pub const fn previous_event_hash(&self) -> Sha256Digest {
        self.previous_event_hash
    }

    /// Returns the event hash.
    #[must_use]
    pub const fn event_hash(&self) -> Sha256Digest {
        self.event_hash
    }

    /// Returns the producing command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }

    /// Borrows exact complete B3 frame bytes.
    #[must_use]
    pub fn frame_bytes(&self) -> &[u8] {
        self.frame.bytes()
    }

    /// Returns the checked B3 frame-family tag.
    #[must_use]
    pub const fn frame_family(&self) -> u16 {
        self.frame.family()
    }

    /// Returns the complete-frame digest.
    #[must_use]
    pub const fn frame_digest(&self) -> Sha256Digest {
        self.frame.digest()
    }

    /// Returns the bound revision digest.
    #[must_use]
    pub const fn revision_digest(&self) -> Sha256Digest {
        self.revision_digest
    }

    /// Borrows canonical causal identities.
    #[must_use]
    pub fn causal_parents(&self) -> &[EventId] {
        &self.causal_parents
    }
}
