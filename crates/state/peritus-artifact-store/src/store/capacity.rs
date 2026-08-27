//! Filesystem capacity observation for the artifact store root.

use super::ArtifactStore;
use crate::{ArtifactStoreError, StoreOperation, path::io};

/// Filesystem capacity observed at the canonical store root.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpaceObservation {
    available_bytes: u64,
    free_bytes: u64,
    total_bytes: u64,
    allocation_granularity: u64,
}

impl SpaceObservation {
    /// Returns bytes available to an unprivileged process.
    #[must_use]
    pub const fn available_bytes(self) -> u64 {
        self.available_bytes
    }

    /// Returns all free filesystem bytes.
    #[must_use]
    pub const fn free_bytes(self) -> u64 {
        self.free_bytes
    }

    /// Returns total filesystem bytes.
    #[must_use]
    pub const fn total_bytes(self) -> u64 {
        self.total_bytes
    }

    /// Returns filesystem allocation granularity.
    #[must_use]
    pub const fn allocation_granularity(self) -> u64 {
        self.allocation_granularity
    }
}

impl ArtifactStore {
    /// Observes capacity without making quota or acceptance decisions.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when filesystem statistics are unavailable.
    pub fn observe_space(&self) -> Result<SpaceObservation, ArtifactStoreError> {
        let stats = fs4::statvfs(self.paths.root())
            .map_err(|error| io(StoreOperation::ObserveSpace, error))?;
        Ok(SpaceObservation {
            available_bytes: stats.available_space(),
            free_bytes: stats.free_space(),
            total_bytes: stats.total_space(),
            allocation_granularity: stats.allocation_granularity(),
        })
    }
}
