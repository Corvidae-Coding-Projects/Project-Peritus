//! Durable generation observations and startup repair plans.

#![allow(
    clippy::redundant_pub_crate,
    reason = "the private module exposes its planner to the sibling SQLite adapter"
)]

use crate::{
    Checkpoint, ProjectionError, ProjectionErrorKind, ProjectionIdentity, ProjectionSchema,
    RecoveryClass,
};
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
    frontier_digest: Option<Sha256Digest>,
    frontier_payload: Option<Vec<u8>>,
    metadata_digest: Option<Sha256Digest>,
}

impl ActiveGeneration {
    pub(crate) fn new(
        generation: CatalogGeneration,
        checkpoint: Checkpoint,
        invariant_digest: Sha256Digest,
        record_count: u64,
        payload: Vec<u8>,
        frontier: Option<(Sha256Digest, Vec<u8>)>,
        metadata_digest: Option<Sha256Digest>,
    ) -> Self {
        let (frontier_digest, frontier_payload) = frontier
            .map_or((None, None), |(digest, payload)| (Some(digest), Some(payload)));
        Self {
            generation,
            checkpoint,
            invariant_digest,
            record_count,
            payload,
            frontier_digest,
            frontier_payload,
            metadata_digest,
        }
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

    /// Borrows the canonical aggregate frontier when this generation predates no migration.
    #[must_use]
    pub fn frontier_payload(&self) -> Option<&[u8]> {
        self.frontier_payload.as_deref()
    }

    /// Returns whether the optional aggregate frontier is present and digest-bound.
    #[must_use]
    pub fn frontier_is_valid(&self) -> bool {
        match (self.frontier_digest, self.frontier_payload.as_deref()) {
            (Some(digest), Some(payload)) => sha256(payload) == digest,
            _ => false,
        }
    }

    pub(crate) const fn metadata_binding_is_present(&self) -> bool {
        self.metadata_digest.is_some()
    }

    pub(crate) fn metadata_is_valid(&self) -> bool {
        let Some(stored) = self.metadata_digest else {
            return false;
        };
        if !self.payload_is_valid()
            || self.record_count != self.checkpoint.last_position()
            || !self.frontier_is_valid()
        {
            return false;
        }
        generation_metadata_digest(
            self.checkpoint.schema().identity(),
            self.generation,
            self.checkpoint.last_position(),
            self.checkpoint.journal_head_digest(),
            self.checkpoint.payload_digest(),
            self.checkpoint.schema().digest(),
            self.invariant_digest,
            self.record_count,
            self.frontier_digest,
        ) == stored
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
    /// The active catalog row has malformed derived metadata.
    CatalogCorrupt,
    /// A pre-binding catalog generation requires checked migration.
    LegacyCheckpoint,
    /// Complete checkpoint metadata does not match its durable binding.
    CheckpointMetadataCorrupt,
}

/// Deterministic startup repair decision.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RepairAction {
    /// The active generation is exactly current.
    Reuse(CatalogGeneration),
    /// Restore the typed active state and apply only verified records after its accepted frontier.
    CatchUpFromCheckpoint(CatalogGeneration),
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
    let metadata_matches = active.metadata_is_valid();
    if crate::verified::checkpoint_current(
        checkpoint.last_position(),
        journal_position,
        head_matches,
        payload_matches,
        schema_matches,
    ) && metadata_matches
    {
        RepairAction::Reuse(active.generation())
    } else if schema_matches
        && payload_matches
        && metadata_matches
        && journal_position > checkpoint.last_position()
        && active.frontier_is_valid()
    {
        RepairAction::CatchUpFromCheckpoint(active.generation())
    } else if !schema_matches {
        RepairAction::RebuildFromGenesis(RepairReason::SchemaChanged)
    } else if !payload_matches {
        RepairAction::RebuildFromGenesis(RepairReason::PayloadCorrupt)
    } else if !active.metadata_binding_is_present() {
        RepairAction::RebuildFromGenesis(RepairReason::LegacyCheckpoint)
    } else if !metadata_matches {
        RepairAction::RebuildFromGenesis(RepairReason::CheckpointMetadataCorrupt)
    } else if !position_matches {
        RepairAction::RebuildFromGenesis(RepairReason::PositionChanged)
    } else {
        RepairAction::RebuildFromGenesis(RepairReason::JournalHeadChanged)
    }
}

pub(crate) fn generation_metadata_digest(
    identity: &ProjectionIdentity,
    generation: CatalogGeneration,
    last_position: u64,
    journal_head_digest: Sha256Digest,
    payload_digest: Sha256Digest,
    schema_digest: Sha256Digest,
    invariant_digest: Sha256Digest,
    record_count: u64,
    frontier_digest: Option<Sha256Digest>,
) -> Sha256Digest {
    let mut bytes = b"peritus-projection-checkpoint-metadata-v1\0".to_vec();
    bytes.extend_from_slice(
        &u64::try_from(identity.name().as_str().len()).unwrap_or(u64::MAX).to_be_bytes(),
    );
    bytes.extend_from_slice(identity.name().as_str().as_bytes());
    bytes.extend_from_slice(&identity.version().get().to_be_bytes());
    bytes.extend_from_slice(&generation.get().to_be_bytes());
    bytes.extend_from_slice(&last_position.to_be_bytes());
    bytes.extend_from_slice(journal_head_digest.as_bytes());
    bytes.extend_from_slice(payload_digest.as_bytes());
    bytes.extend_from_slice(schema_digest.as_bytes());
    bytes.extend_from_slice(invariant_digest.as_bytes());
    bytes.extend_from_slice(&record_count.to_be_bytes());
    match frontier_digest {
        Some(digest) => {
            bytes.push(1);
            bytes.extend_from_slice(digest.as_bytes());
        }
        None => bytes.push(0),
    }
    sha256(&bytes)
}
