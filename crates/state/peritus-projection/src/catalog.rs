//! Durable generation observations and startup repair plans.

#![allow(
    clippy::redundant_pub_crate,
    reason = "the private module exposes its planner to the sibling SQLite adapter"
)]

use crate::{Checkpoint, ProjectionError, ProjectionErrorKind, ProjectionSchema, RecoveryClass};
use peritus_codec::sha256;
use peritus_types::Sha256Digest;
use std::num::NonZeroU64;

/// Positive durable projection generation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogGeneration(NonZeroU64);

impl CatalogGeneration {
    /// Creates a positive generation.
    #[must_use]
    pub const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }

    /// Returns the positive generation number.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    pub(crate) fn from_u64(value: u64) -> Result<Self, ProjectionError> {
        NonZeroU64::new(value).map(Self).ok_or_else(|| {
            ProjectionError::new(
                ProjectionErrorKind::CorruptCatalog,
                RecoveryClass::Rebuild,
                "read projection generation",
                "stored generation is zero",
            )
        })
    }
}

/// One active durable projection generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActiveGeneration {
    generation: CatalogGeneration,
    checkpoint: Checkpoint,
    invariant_digest: Sha256Digest,
    record_count: u64,
    payload: Vec<u8>,
}

impl ActiveGeneration {
    pub(crate) const fn new(
        generation: CatalogGeneration,
        checkpoint: Checkpoint,
        invariant_digest: Sha256Digest,
        record_count: u64,
        payload: Vec<u8>,
    ) -> Self {
        Self { generation, checkpoint, invariant_digest, record_count, payload }
    }

    /// Returns the active generation number.
    #[must_use]
    pub const fn generation(&self) -> CatalogGeneration {
        self.generation
    }

    /// Borrows the bound checkpoint.
    #[must_use]
    pub const fn checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }

    /// Returns the independently stored invariant checksum.
    #[must_use]
    pub const fn invariant_digest(&self) -> Sha256Digest {
        self.invariant_digest
    }

    /// Returns the replayed record count.
    #[must_use]
    pub const fn record_count(&self) -> u64 {
        self.record_count
    }

    /// Borrows the exact deterministic payload.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Returns whether the durable bytes still match their checkpoint digest.
    #[must_use]
    pub fn payload_is_valid(&self) -> bool {
        self.checkpoint.binds_payload(&self.payload)
    }
}

/// Why startup cannot safely reuse an active generation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RepairReason {
    /// No active generation exists.
    Missing,
    /// The projection implementation schema changed.
    SchemaChanged,
    /// The journal advanced or rewound.
    PositionChanged,
    /// Journal history at the checkpoint differs.
    JournalHeadChanged,
    /// Durable payload bytes no longer match their digest.
    PayloadCorrupt,
}

/// Deterministic startup repair decision.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RepairAction {
    /// The active generation is exactly current.
    Reuse(CatalogGeneration),
    /// Rebuild a new shadow generation from journal genesis.
    RebuildFromGenesis(RepairReason),
}

pub(super) fn plan_repair(
    active: Option<&ActiveGeneration>,
    schema: &ProjectionSchema,
    journal_position: u64,
    journal_head: Sha256Digest,
) -> RepairAction {
    let Some(active) = active else {
        return RepairAction::RebuildFromGenesis(RepairReason::Missing);
    };
    let checkpoint = active.checkpoint();
    let schema_matches = checkpoint.schema().digest() == schema.digest();
    let position_matches = checkpoint.last_position() == journal_position;
    let head_matches = checkpoint.journal_head_digest() == journal_head;
    let payload_matches = checkpoint.payload_digest() == sha256(active.payload());
    if crate::verified::checkpoint_current(
        checkpoint.last_position(),
        journal_position,
        head_matches,
        payload_matches,
        schema_matches,
    ) {
        RepairAction::Reuse(active.generation())
    } else if !schema_matches {
        RepairAction::RebuildFromGenesis(RepairReason::SchemaChanged)
    } else if !payload_matches {
        RepairAction::RebuildFromGenesis(RepairReason::PayloadCorrupt)
    } else if !position_matches {
        RepairAction::RebuildFromGenesis(RepairReason::PositionChanged)
    } else {
        RepairAction::RebuildFromGenesis(RepairReason::JournalHeadChanged)
    }
}
