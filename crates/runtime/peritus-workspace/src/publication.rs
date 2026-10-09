//! Atomic ownership of retained Git snapshots and their durable workspace manifests.

use std::fmt;

use peritus_artifact_store::{ArtifactStore, FinalizedArtifact};
use peritus_git::{CandidateSnapshot, GitError, GitRepository};
use peritus_types::EventId;

use crate::{WorkspaceError, WorkspaceManifest};

/// Artifact failure plus the result of compensating an already-retained Git snapshot.
#[derive(Debug)]
pub struct SnapshotPublicationFailure {
    artifact: WorkspaceError,
    compensation: Option<GitError>,
}

impl SnapshotPublicationFailure {
    /// Returns the manifest-store failure that started compensation.
    #[must_use]
    pub const fn artifact_failure(&self) -> &WorkspaceError {
        &self.artifact
    }

    /// Returns a Git failure only when the retained reference could not be released exactly.
    #[must_use]
    pub const fn compensation_failure(&self) -> Option<&GitError> {
        self.compensation.as_ref()
    }
}

impl fmt::Display for SnapshotPublicationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.compensation {
            Some(compensation) => write!(
                formatter,
                "workspace manifest storage failed: {}; retained snapshot cleanup failed: {}",
                self.artifact, compensation
            ),
            None => write!(
                formatter,
                "workspace manifest storage failed and the retained snapshot was released: {}",
                self.artifact
            ),
        }
    }
}

impl std::error::Error for SnapshotPublicationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.artifact)
    }
}

/// Finalizes the manifest or compensates the already-retained snapshot reference.
///
/// # Errors
///
/// Returns both the manifest-store failure and any failure to release the exact retained Git
/// reference. A missing compensation failure means the snapshot reference was removed.
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
            Err(SnapshotPublicationFailure { artifact, compensation })
        }
    }
}
