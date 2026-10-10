//! Deletion tombstones that dominate replay without retaining deleted content.

use crate::{DeletionReason, MemoryId, MemoryRecord, Observation};
use peritus_types::{RevisionNumber, Sha256Digest};

#[doc = " Immutable deletion marker for one memory lineage."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryTombstone {
    memory_id: MemoryId,
    last_known_revision: RevisionNumber,
    deletion_observation: Observation,
    reason: DeletionReason,
    prior_digest: Sha256Digest,
}
impl MemoryTombstone {
    #[doc = " Imports a checked deletion marker from already validated domain values."]
    #[must_use]
    pub const fn new(
        memory_id: MemoryId,
        last_known_revision: RevisionNumber,
        deletion_observation: Observation,
        reason: DeletionReason,
        prior_digest: Sha256Digest,
    ) -> Self {
        Self { memory_id, last_known_revision, deletion_observation, reason, prior_digest }
    }
    #[doc = " Returns the deleted memory identifier."]
    #[must_use]
    pub const fn memory_id(&self) -> MemoryId {
        self.memory_id
    }
    #[doc = " Returns the highest revision suppressed by this deletion."]
    #[must_use]
    pub const fn last_known_revision(&self) -> RevisionNumber {
        self.last_known_revision
    }
    #[doc = " Returns the caller-supplied logical deletion observation."]
    #[must_use]
    pub const fn deletion_observation(&self) -> Observation {
        self.deletion_observation
    }
    #[doc = " Returns the durable deletion reason."]
    #[must_use]
    pub const fn reason(&self) -> DeletionReason {
        self.reason
    }
    #[doc = " Returns the digest of the content that was forgotten."]
    #[must_use]
    pub const fn prior_digest(&self) -> Sha256Digest {
        self.prior_digest
    }
    #[doc = " Returns whether deletion wins over this record under replay."]
    #[must_use]
    pub const fn dominates(&self, record: &MemoryRecord) -> bool {
        let tombstone_id = self.memory_id();
        let record_id = record.id();
        let tombstone_revision = self.last_known_revision();
        let record_revision = record.revision();
        let id_matches = tombstone_id.same_identity(&record_id);
        let tombstone_value = tombstone_revision.get();
        let record_value = record_revision.get();
        let revision_matches = tombstone_value >= record_value;
        id_matches && revision_matches
    }
}
