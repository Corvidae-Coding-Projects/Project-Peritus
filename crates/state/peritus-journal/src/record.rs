//! Exact frame records and checked state-install inputs.

mod committed;
mod state;

use crate::{AggregateKey, JournalError, JournalErrorKind};
use peritus_codec::{CodecLimits, decode_frame, sha256};
use peritus_types::{EventId, EventSequence, Sha256Digest};

pub use committed::CommittedRecord;
pub use state::MAX_STATE_KEY_BYTES;
pub use state::{DurableStateRecord, StateInstall};

/// Maximum exact records returned by one global event query.
pub const MAX_GLOBAL_WINDOW_RECORDS: usize = 4_096;

/// One bounded, snapshot-consistent window over store-wide event positions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalEventWindow {
    earliest: u64,
    latest: u64,
    records: Vec<CommittedRecord>,
}

impl GlobalEventWindow {
    pub(crate) const fn new(earliest: u64, latest: u64, records: Vec<CommittedRecord>) -> Self {
        Self { earliest, latest, records }
    }

    /// Returns the oldest retained global position, or zero when the journal is empty.
    #[must_use]
    pub const fn earliest(&self) -> u64 {
        self.earliest
    }

    /// Returns the newest retained global position, or zero when the journal is empty.
    #[must_use]
    pub const fn latest(&self) -> u64 {
        self.latest
    }

    /// Borrows exact immutable records in strictly increasing global-position order.
    #[must_use]
    pub fn records(&self) -> &[CommittedRecord] {
        &self.records
    }

    /// Reports whether resuming after `cursor` requires a retention-gap response.
    #[must_use]
    pub const fn has_retention_gap_after(&self, cursor: u64) -> bool {
        self.earliest > 0 && cursor.saturating_add(1) < self.earliest
    }
}

/// Maximum causal parents on one event.
pub const MAX_CAUSAL_PARENTS: usize = 4_096;

/// A checked complete B3 canonical frame retained without reserialization.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ExactFrame {
    bytes: Vec<u8>,
    family: u16,
    schema_version: u16,
    digest: Sha256Digest,
}

impl ExactFrame {
    /// Validates and owns complete canonical frame bytes.
    ///
    /// # Errors
    ///
    /// Returns an invalid-input error when the complete B3 frame fails canonical framing or the
    /// production frame-size bound.
    pub fn new(bytes: Vec<u8>) -> Result<Self, JournalError> {
        let checked = decode_frame(&bytes, CodecLimits::PRODUCTION).map_err(|_| {
            JournalError::new(
                JournalErrorKind::InvalidInput,
                "validate event frame",
                "frame is not a complete canonical B3 frame",
            )
        })?;
        let family = checked.header().family();
        let schema_version = checked.header().schema_version();
        let digest = sha256(&bytes);
        Ok(Self { bytes, family, schema_version, digest })
    }

    /// Borrows the exact complete frame bytes supplied by the caller.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the frame family from its checked header.
    #[must_use]
    pub const fn family(&self) -> u16 {
        self.family
    }

    /// Returns the family schema version from its checked header.
    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    /// Returns SHA-256 over the complete exact frame.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// One caller-planned immutable aggregate event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventDraft {
    aggregate: AggregateKey,
    sequence: EventSequence,
    event_id: EventId,
    previous_event_id: Option<EventId>,
    frame: ExactFrame,
    revision_digest: Sha256Digest,
    causal_parents: Vec<EventId>,
}

impl EventDraft {
    /// Creates a bounded event draft and validates canonical causal-parent ordering.
    ///
    /// # Errors
    ///
    /// Returns a typed input error for too many, duplicate, or noncanonically ordered parents.
    #[allow(clippy::too_many_arguments, reason = "journal hash inputs remain explicit")]
    pub fn new(
        aggregate: AggregateKey,
        sequence: EventSequence,
        event_id: EventId,
        previous_event_id: Option<EventId>,
        frame: ExactFrame,
        revision_digest: Sha256Digest,
        causal_parents: Vec<EventId>,
    ) -> Result<Self, JournalError> {
        if causal_parents.len() > MAX_CAUSAL_PARENTS {
            return Err(JournalError::new(
                JournalErrorKind::InvalidInput,
                "validate event",
                "too many causal parents",
            ));
        }
        for pair in causal_parents.windows(2) {
            if pair[0] == pair[1] {
                return Err(JournalError::new(
                    JournalErrorKind::DuplicateIdentity,
                    "validate event",
                    "duplicate causal parent",
                ));
            }
            if pair[0] > pair[1] {
                return Err(JournalError::new(
                    JournalErrorKind::NonCanonicalOrder,
                    "validate event",
                    "causal parents must be strictly ordered",
                ));
            }
        }
        Ok(Self {
            aggregate,
            sequence,
            event_id,
            previous_event_id,
            frame,
            revision_digest,
            causal_parents,
        })
    }

    /// Returns the aggregate key.
    #[must_use]
    pub const fn aggregate(&self) -> AggregateKey {
        self.aggregate
    }

    /// Returns the declared one-based sequence.
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

    /// Borrows the exact checked frame.
    #[must_use]
    pub const fn frame(&self) -> &ExactFrame {
        &self.frame
    }

    /// Returns the revision digest bound into the hash chain.
    #[must_use]
    pub const fn revision_digest(&self) -> Sha256Digest {
        self.revision_digest
    }

    /// Borrows canonical causal predecessor identities.
    #[must_use]
    pub fn causal_parents(&self) -> &[EventId] {
        &self.causal_parents
    }
}

/// One finalized content digest that must exist before append.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ArtifactDependency {
    digest: Sha256Digest,
}

impl ArtifactDependency {
    /// Creates a dependency on one exact finalized digest.
    #[must_use]
    pub const fn new(digest: Sha256Digest) -> Self {
        Self { digest }
    }

    /// Returns the required digest.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
}
