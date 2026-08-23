//! Deterministic mark, quarantine, restore, and sweep plans.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    ArtifactDigest, ArtifactStoreError, ErrorCode, QuarantineState, RecoveryClass, ReferenceRoots,
    verified::sweep_is_later,
};

/// Positive collection generation used to separate quarantine from deletion.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CollectionGeneration(u64);

impl CollectionGeneration {
    /// Creates a positive collection generation.
    ///
    /// # Errors
    ///
    /// Returns an error when `value` is zero.
    pub const fn new(value: u64) -> Result<Self, ArtifactStoreError> {
        if value == 0 {
            Err(invalid_plan("collection generation must be positive"))
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the primitive generation value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Finalized artifact observation consumed by the pure collection planner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GcInventoryEntry {
    digest: ArtifactDigest,
    size: u64,
    quarantine: QuarantineState,
}

impl GcInventoryEntry {
    /// Creates an inventory observation.
    #[must_use]
    pub const fn new(digest: ArtifactDigest, size: u64, quarantine: QuarantineState) -> Self {
        Self { digest, size, quarantine }
    }

    /// Returns the digest.
    #[must_use]
    pub const fn digest(self) -> ArtifactDigest {
        self.digest
    }

    /// Returns logical bytes.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }

    /// Returns observed quarantine state.
    #[must_use]
    pub const fn quarantine(self) -> QuarantineState {
        self.quarantine
    }
}

/// One deterministic filesystem and metadata transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GcAction {
    /// Move an unmarked active object into quarantine.
    Quarantine {
        /// Artifact to move.
        digest: ArtifactDigest,
        /// Exact expected logical size.
        size: u64,
        /// Generation assigned to quarantine metadata.
        generation: CollectionGeneration,
    },
    /// Restore a quarantined object that became marked again.
    Restore {
        /// Artifact to restore.
        digest: ArtifactDigest,
        /// Exact expected logical size.
        size: u64,
        /// Prior quarantine generation.
        since: CollectionGeneration,
    },
    /// Delete an unmarked object quarantined by an earlier generation.
    Delete {
        /// Artifact to delete.
        digest: ArtifactDigest,
        /// Exact expected logical size.
        size: u64,
        /// Prior quarantine generation.
        since: CollectionGeneration,
    },
}

impl GcAction {
    /// Returns the action digest.
    #[must_use]
    pub const fn digest(self) -> ArtifactDigest {
        match self {
            Self::Quarantine { digest, .. }
            | Self::Restore { digest, .. }
            | Self::Delete { digest, .. } => digest,
        }
    }
}

/// Canonically ordered collection plan bound to one generation and inventory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GcPlan {
    generation: CollectionGeneration,
    actions: Vec<GcAction>,
    marked: BTreeSet<ArtifactDigest>,
}

impl GcPlan {
    /// Computes a deterministic mark-and-sweep plan without performing I/O.
    ///
    /// An active unmarked artifact is quarantined. An unmarked artifact already quarantined by a
    /// strictly earlier generation is deleted. A marked quarantined artifact is restored. The
    /// planner rejects duplicate inventory rows and references to missing artifacts.
    ///
    /// # Errors
    ///
    /// Returns an invalid-plan error for duplicate inventory, future quarantine generations, or
    /// missing roots.
    pub fn build(
        generation: CollectionGeneration,
        inventory: impl IntoIterator<Item = GcInventoryEntry>,
        roots: &ReferenceRoots,
    ) -> Result<Self, ArtifactStoreError> {
        let mut entries = BTreeMap::new();
        for entry in inventory {
            if entries.insert(entry.digest, entry).is_some() {
                return Err(invalid_plan("duplicate artifact in collection inventory"));
            }
        }
        let marked = roots.all();
        if marked.iter().any(|digest| !entries.contains_key(digest)) {
            return Err(invalid_plan("a collection root names a missing artifact"));
        }
        let mut actions = Vec::new();
        for entry in entries.into_values() {
            let is_marked = marked.contains(&entry.digest);
            match (entry.quarantine, is_marked) {
                (QuarantineState::Active, false) => actions.push(GcAction::Quarantine {
                    digest: entry.digest,
                    size: entry.size,
                    generation,
                }),
                (QuarantineState::Quarantined { since }, true) => {
                    actions.push(GcAction::Restore {
                        digest: entry.digest,
                        size: entry.size,
                        since,
                    });
                }
                (QuarantineState::Quarantined { since }, false) => {
                    if since > generation {
                        return Err(invalid_plan("quarantine generation is newer than the plan"));
                    }
                    if sweep_is_later(since.get(), generation.get()) {
                        actions.push(GcAction::Delete {
                            digest: entry.digest,
                            size: entry.size,
                            since,
                        });
                    }
                }
                (QuarantineState::Active, true) => {}
            }
        }
        Ok(Self { generation, actions, marked })
    }

    /// Returns the plan generation.
    #[must_use]
    pub const fn generation(&self) -> CollectionGeneration {
        self.generation
    }

    /// Returns actions in canonical digest order.
    #[must_use]
    pub fn actions(&self) -> &[GcAction] {
        &self.actions
    }

    /// Returns the canonical marked set.
    #[must_use]
    pub const fn marked(&self) -> &BTreeSet<ArtifactDigest> {
        &self.marked
    }
}

/// Summary of one completely applied collection plan.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GcApplication {
    quarantined: u64,
    restored: u64,
    deleted: u64,
    deleted_bytes: u64,
}

impl GcApplication {
    pub(crate) fn observe(&mut self, action: GcAction) -> Result<(), ArtifactStoreError> {
        match action {
            GcAction::Quarantine { .. } => {
                self.quarantined = self.quarantined.checked_add(1).ok_or_else(overflow)?;
            }
            GcAction::Restore { .. } => {
                self.restored = self.restored.checked_add(1).ok_or_else(overflow)?;
            }
            GcAction::Delete { size, .. } => {
                self.deleted = self.deleted.checked_add(1).ok_or_else(overflow)?;
                self.deleted_bytes = self.deleted_bytes.checked_add(size).ok_or_else(overflow)?;
            }
        }
        Ok(())
    }

    /// Returns the number moved into quarantine.
    #[must_use]
    pub const fn quarantined(self) -> u64 {
        self.quarantined
    }

    /// Returns the number restored to active objects.
    #[must_use]
    pub const fn restored(self) -> u64 {
        self.restored
    }

    /// Returns the number deleted from quarantine.
    #[must_use]
    pub const fn deleted(self) -> u64 {
        self.deleted
    }

    /// Returns logical bytes deleted.
    #[must_use]
    pub const fn deleted_bytes(self) -> u64 {
        self.deleted_bytes
    }
}

const fn invalid_plan(message: &'static str) -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::InvalidCollectionPlan,
        RecoveryClass::CorrectRequest,
        message,
    )
}

const fn overflow() -> ArtifactStoreError {
    ArtifactStoreError::message(
        ErrorCode::ArithmeticOverflow,
        RecoveryClass::RecoverStore,
        "collection application accounting overflowed",
    )
}
