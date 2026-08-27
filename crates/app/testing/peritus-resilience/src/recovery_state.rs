//! Grouped recovered-state and accounting inputs.

use crate::{
    AcceptanceObservation, ArtifactHealth, CorruptionObservation, JournalHealth,
    OwnershipObservation, ProjectionHealth, ResourceUsage, RetryUsage,
};

/// Integrity, admission, and acceptance facts grouped as one recovered state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveredStateObservation {
    pub(super) acceptance: AcceptanceObservation,
    pub(super) journal: JournalHealth,
    pub(super) artifacts: ArtifactHealth,
    pub(super) projection: ProjectionHealth,
    pub(super) corruption: CorruptionObservation,
    pub(super) temporary_objects: u16,
}

impl RecoveredStateObservation {
    /// Creates direct recovered-state facts.
    #[must_use]
    pub const fn new(
        acceptance: AcceptanceObservation,
        journal: JournalHealth,
        artifacts: ArtifactHealth,
        projection: ProjectionHealth,
        corruption: CorruptionObservation,
        temporary_objects: u16,
    ) -> Self {
        Self { acceptance, journal, artifacts, projection, corruption, temporary_objects }
    }
}

/// Ownership, retry, and resource accounting grouped for one recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecoveryAccounting {
    pub(super) ownership: OwnershipObservation,
    pub(super) retries: RetryUsage,
    pub(super) resources: ResourceUsage,
}

impl RecoveryAccounting {
    /// Creates direct recovery accounting.
    #[must_use]
    pub const fn new(
        ownership: OwnershipObservation,
        retries: RetryUsage,
        resources: ResourceUsage,
    ) -> Self {
        Self { ownership, retries, resources }
    }
}
