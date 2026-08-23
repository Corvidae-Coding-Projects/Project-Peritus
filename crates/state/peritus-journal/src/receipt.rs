//! Opaque move-only committed observations.

use crate::{AggregateHead, ArtifactDependency, CommittedRecord, ExactFrame};
use peritus_types::{CommandId, Sha256Digest};

/// Opaque exact post-commit observation of one atomic event batch.
///
/// This value has private fields, no public constructor, and is deliberately neither `Clone` nor
/// `Copy`. It is evidence of a journal observation, not permission to perform an external effect.
#[derive(Debug, Eq, PartialEq)]
pub struct CommittedBatch {
    pub(crate) command_id: CommandId,
    pub(crate) request_digest: Sha256Digest,
    pub(crate) first_position: u64,
    pub(crate) last_position: u64,
    pub(crate) batch_hash: Sha256Digest,
    pub(crate) records: Vec<CommittedRecord>,
    pub(crate) heads: Vec<AggregateHead>,
    pub(crate) artifact_dependencies: Vec<ArtifactDependency>,
}

impl CommittedBatch {
    /// Returns the committed command identity.
    #[must_use]
    pub const fn command_id(&self) -> CommandId {
        self.command_id
    }

    /// Returns the exact request digest bound by idempotency.
    #[must_use]
    pub const fn request_digest(&self) -> Sha256Digest {
        self.request_digest
    }

    /// Returns the first one-based global event position.
    #[must_use]
    pub const fn first_position(&self) -> u64 {
        self.first_position
    }

    /// Returns the last one-based global event position.
    #[must_use]
    pub const fn last_position(&self) -> u64 {
        self.last_position
    }

    /// Returns the deterministic batch hash.
    #[must_use]
    pub const fn batch_hash(&self) -> Sha256Digest {
        self.batch_hash
    }

    /// Borrows checked committed records in global-position order.
    #[must_use]
    pub fn records(&self) -> &[CommittedRecord] {
        &self.records
    }

    /// Borrows exact affected aggregate heads in canonical key order.
    #[must_use]
    pub fn heads(&self) -> &[AggregateHead] {
        &self.heads
    }

    /// Borrows finalized artifact dependencies in canonical digest order.
    #[must_use]
    pub fn artifact_dependencies(&self) -> &[ArtifactDependency] {
        &self.artifact_dependencies
    }
}

/// Opaque exact observation of the durable current credential-registry row.
#[derive(Debug, Eq, PartialEq)]
pub struct CurrentCredentialRegistry {
    pub(crate) revision: u64,
    pub(crate) generation: u64,
    pub(crate) digest: Sha256Digest,
    pub(crate) snapshot: ExactFrame,
    pub(crate) producing_position: u64,
}

impl CurrentCredentialRegistry {
    /// Returns the positive current registry revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the positive current credential generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the digest of the exact canonical checked snapshot payload.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Borrows exact complete registry snapshot-frame bytes.
    #[must_use]
    pub fn snapshot_bytes(&self) -> &[u8] {
        self.snapshot.bytes()
    }

    /// Returns the producing event position observed with this row.
    #[must_use]
    pub const fn producing_position(&self) -> u64 {
        self.producing_position
    }
}
