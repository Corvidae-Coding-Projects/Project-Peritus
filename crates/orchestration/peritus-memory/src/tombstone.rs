//! Deletion tombstones that dominate replay without retaining deleted content.

use crate::{DeletionReason, MemoryId, MemoryRecord, Observation};
use peritus_types::{RevisionNumber, Sha256Digest};
use vstd::prelude::*;

verus! {

/// Immutable deletion marker for one memory lineage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryTombstone {
    memory_id: MemoryId,
    last_known_revision: RevisionNumber,
    deletion_observation: Observation,
    reason: DeletionReason,
    prior_digest: Sha256Digest,
}

impl MemoryTombstone {
    /// Imports a checked deletion marker from already validated domain values.
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

    /// Returns the deleted memory identifier.
    #[must_use]
    pub const fn memory_id(&self) -> (result: MemoryId)
        ensures result.spec_bytes() == self.spec_memory_id_bytes(),
    {
        self.memory_id
    }

    /// Returns the deleted lineage identifier used by specifications.
    pub closed spec fn spec_memory_id_bytes(&self) -> Seq<u8> {
        self.memory_id.spec_bytes()
    }

    /// Returns the highest revision suppressed by this deletion.
    #[must_use]
    pub const fn last_known_revision(&self) -> (result: RevisionNumber)
        ensures result.spec_value() == self.spec_last_known_revision(),
    {
        self.last_known_revision
    }

    /// Returns the mathematical deletion revision bound used by specifications.
    pub closed spec fn spec_last_known_revision(&self) -> int {
        self.last_known_revision.spec_value()
    }

    /// Returns the caller-supplied logical deletion observation.
    #[must_use]
    pub const fn deletion_observation(&self) -> Observation { self.deletion_observation }

    /// Returns the durable deletion reason.
    #[must_use]
    pub const fn reason(&self) -> DeletionReason { self.reason }

    /// Returns the digest of the content that was forgotten.
    #[must_use]
    pub const fn prior_digest(&self) -> Sha256Digest { self.prior_digest }

    /// Returns whether deletion wins over this record under replay.
    #[must_use]
    pub const fn dominates(&self, record: &MemoryRecord) -> (result: bool)
        ensures result == self.spec_dominates(record),
    {
        let tombstone_id = self.memory_id();
        let record_id = record.id();
        let tombstone_revision = self.last_known_revision();
        let record_revision = record.revision();
        assert(tombstone_id.spec_bytes() == self.spec_memory_id_bytes());
        assert(record_id.spec_bytes() == record.spec_id_bytes());
        assert(tombstone_revision.spec_value() == self.spec_last_known_revision());
        assert(record_revision.spec_value() == record.spec_revision_value());
        let id_matches = tombstone_id.same_identity(&record_id);
        let tombstone_value = tombstone_revision.get();
        let record_value = record_revision.get();
        let revision_matches = tombstone_value >= record_value;
        assert(id_matches == (self.spec_memory_id_bytes() == record.spec_id_bytes()));
        assert((tombstone_value as int) == self.spec_last_known_revision());
        assert((record_value as int) == record.spec_revision_value());
        assert(revision_matches
            == (self.spec_last_known_revision() >= record.spec_revision_value()));
        id_matches && revision_matches
    }

    /// Returns exact tombstone dominance as a mathematical predicate.
    pub open spec fn spec_dominates(&self, record: &MemoryRecord) -> bool {
        self.spec_memory_id_bytes() == record.spec_id_bytes()
            && self.spec_last_known_revision() >= record.spec_revision_value()
    }
}

} // verus!
