//! Atomic ownership of retained Git snapshots and their durable workspace manifests.

use std::fmt;

use peritus_artifact_store::{ArtifactStore, FinalizedArtifact};
use peritus_git::{CandidateSnapshot, GitError, GitRepository};
use peritus_types::EventId;

use crate::{WorkspaceError, WorkspaceManifest};

/// Artifact failure while the Git snapshot remains retained for exact recovery.
#[derive(Debug)]
pub struct SnapshotPublicationFailure {
    artifact: WorkspaceError,
    compensation: Option<GitError>,
    retained: bool,
}

impl SnapshotPublicationFailure {
    /// Returns the manifest-store failure.
    #[must_use]
    pub const fn artifact_failure(&self) -> &WorkspaceError {
        &self.artifact
    }

    /// Returns whether the exact Git snapshot remains available for recovery.
    #[must_use]
    pub const fn snapshot_retained(&self) -> bool {
        self.retained
    }

    /// Returns a Git failure only when the compensating release could not be proven.
    #[must_use]
    pub const fn compensation_failure(&self) -> Option<&GitError> {
        self.compensation.as_ref()
    }
}

impl fmt::Display for SnapshotPublicationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.retained {
            write!(
                formatter,
                "workspace manifest storage failed; retained snapshot is recoverable: {}",
                self.artifact
            )
        } else {
            write!(
                formatter,
                "workspace manifest storage failed and snapshot was released: {}",
                self.artifact
            )
        }
    }
}

impl std::error::Error for SnapshotPublicationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.artifact)
    }
}

/// Finalizes the manifest or compensates the exact retained Git reference.
///
/// # Errors
///
/// Returns the manifest-store failure and preserves the established compensation behavior.
pub fn finalize_snapshot_manifest(
    repository: &GitRepository,
    snapshot: &CandidateSnapshot,
    manifest: &WorkspaceManifest,
    artifacts: &ArtifactStore,
    creating_event: EventId,
) -> Result<FinalizedArtifact, SnapshotPublicationFailure> {
    match manifest.finalize(artifacts, creating_event) {
        Ok(artifact) => Ok(artifact),
        Err(artifact) => {
            let compensation = repository.release_snapshot(snapshot).err();
            Err(SnapshotPublicationFailure {
                artifact,
                retained: compensation.is_some(),
                compensation,
            })
        }
    }
}

/// Finalizes a candidate/rollback manifest while preserving its exact snapshot on failure.
///
/// # Errors
/// Returns the manifest-store failure and retains its Git snapshot for action recovery.
pub fn finalize_snapshot_manifest_recoverable(
    _repository: &GitRepository,
    _snapshot: &CandidateSnapshot,
    manifest: &WorkspaceManifest,
    artifacts: &ArtifactStore,
    creating_event: EventId,
) -> Result<FinalizedArtifact, SnapshotPublicationFailure> {
    manifest.finalize(artifacts, creating_event).map_err(|artifact| SnapshotPublicationFailure {
        artifact,
        compensation: None,
        retained: true,
    })
}
