//! Direct recovery-health projections for admitted controller routes.

use super::route::ScenarioRoute;

impl ScenarioRoute {
    /// Returns the directly supportable journal-health observation.
    pub(in crate::native_controller) const fn journal_health(self) -> &'static str {
        match self {
            Self::JournalCorruption => "hash-divergence-detected",
            Self::DaemonLifecycle(_) | Self::JournalAfterDurableCommitBeforeAck => {
                "recovered-and-verified"
            }
            _ => "verified",
        }
    }

    pub(in crate::native_controller) const fn projection_health(self) -> &'static str {
        match self {
            Self::ProjectionCorruption => "rebuilt-and-verified",
            Self::JournalCorruption => "unavailable",
            _ => "verified",
        }
    }

    pub(in crate::native_controller) const fn artifact_health(self) -> &'static str {
        match self {
            Self::BlobCorruption
            | Self::SnapshotCorruption
            | Self::AcceptanceEvidenceCorruption => "divergence-detected",
            _ => "verified",
        }
    }

    pub(in crate::native_controller) const fn corruption_target(self) -> Option<&'static str> {
        match self {
            Self::ProjectionCorruption => Some("projection"),
            Self::BlobCorruption => Some("blob"),
            Self::AcceptanceEvidenceCorruption => Some("acceptance-evidence"),
            Self::SnapshotCorruption => Some("snapshot"),
            Self::JournalCorruption => Some("journal"),
            _ => None,
        }
    }

    pub(in crate::native_controller) const fn mutation_admitted(self) -> bool {
        !matches!(
            self,
            Self::ProjectionCorruption
                | Self::JournalCorruption
                | Self::BlobCorruption
                | Self::AcceptanceEvidenceCorruption
                | Self::SnapshotCorruption
        )
    }
}
