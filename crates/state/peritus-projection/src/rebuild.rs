//! Pure shadow-generation rebuild preparation.

use crate::{Checkpoint, Projection, ProjectionError, ReplayOutput, replay_from_genesis};
use peritus_journal::IntegrityExport;
use peritus_types::Sha256Digest;

/// Fully checked immutable candidate ready for a transactional generation install.
#[derive(Debug)]
pub struct RebuildCandidate<S> {
    output: ReplayOutput<S>,
}

impl<S> RebuildCandidate<S> {
    /// Borrows the completed state for caller-side invariant inspection.
    #[must_use]
    pub const fn state(&self) -> &S {
        self.output.state()
    }

    /// Borrows the exact payload to persist.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        self.output.payload()
    }

    /// Borrows the exact checkpoint binding.
    #[must_use]
    pub const fn checkpoint(&self) -> &Checkpoint {
        self.output.checkpoint()
    }

    /// Returns the independent fold-invariant checksum.
    #[must_use]
    pub const fn invariant_digest(&self) -> Sha256Digest {
        self.output.invariant_digest()
    }

    /// Returns the number of records folded.
    #[must_use]
    pub const fn record_count(&self) -> u64 {
        self.output.record_count()
    }
}

/// Builds and verifies a shadow candidate entirely in memory.
///
/// # Errors
///
/// Returns any checked replay or projection invariant failure.
pub fn rebuild_from_genesis<P: Projection>(
    projection: &P,
    export: &IntegrityExport,
) -> Result<RebuildCandidate<P::State>, ProjectionError> {
    replay_from_genesis(projection, export).map(|output| RebuildCandidate { output })
}
