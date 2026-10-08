//! Effect-free projection fold contracts.

use crate::{ProjectionError, ProjectionSchema};
use peritus_journal::{CommittedRecord, IntegrityExport};
use peritus_types::Sha256Digest;

/// Read-only checked input passed to a pure fold.
///
/// This value has no constructor outside the replay module and carries no connection, clock,
/// network client, callback, or ambient context.
#[derive(Clone, Copy, Debug)]
pub struct FoldContext<'a> {
    pub(crate) record: &'a CommittedRecord,
    pub(crate) family: u16,
    pub(crate) schema_version: u16,
}

impl FoldContext<'_> {
    /// Borrows the immutable journal record.
    #[must_use]
    pub const fn record(&self) -> &CommittedRecord {
        self.record
    }

    /// Returns the checked frame family.
    #[must_use]
    pub const fn family(&self) -> u16 {
        self.family
    }

    /// Returns the checked family schema version.
    #[must_use]
    pub const fn schema_version(&self) -> u16 {
        self.schema_version
    }

    /// Borrows the exact complete frame bytes without reserialization.
    #[must_use]
    pub fn frame_bytes(&self) -> &[u8] {
        self.record.frame_bytes()
    }
}

/// Deterministic projection state encoding and invariant contract.
pub trait ProjectionState: Eq {
    /// Produces the canonical durable payload for this state.
    fn encode(&self) -> Vec<u8>;

    /// Restores a typed state from its canonical durable payload.
    ///
    /// Implementations that do not support durable restoration retain genesis-replay behavior.
    /// Built-in production projections implement this as a canonical, bounded decoder.
    ///
    /// # Errors
    ///
    /// Returns a corrupt-catalog error when bytes cannot restore the exact typed state.
    fn decode(_payload: &[u8]) -> Result<Self, ProjectionError>
    where
        Self: Sized,
    {
        Err(ProjectionError::fold(
            crate::ProjectionErrorKind::CorruptCatalog,
            crate::RecoveryClass::Rebuild,
            "decode projection checkpoint",
            "projection state does not support durable typed restoration",
        ))
    }

    /// Validates whole-state invariants after replay.
    ///
    /// # Errors
    ///
    /// Returns a fold-invariant error when the state is not safe to publish.
    fn validate(&self) -> Result<(), ProjectionError>;

    /// Returns an independent invariant checksum for shadow-generation comparison.
    fn invariant_digest(&self) -> Sha256Digest;
}

/// Pure deterministic fold contract over checked journal records.
pub trait Projection {
    /// Concrete effect-free state.
    type State: ProjectionState;

    /// Returns the stable versioned projection schema.
    fn schema(&self) -> &ProjectionSchema;

    /// Creates the deterministic genesis state.
    fn genesis(&self) -> Self::State;

    /// Applies one checked record without external effects.
    ///
    /// # Errors
    ///
    /// Returns typed frame, revision, or invariant failures.
    fn fold(&self, state: &mut Self::State, input: FoldContext<'_>) -> Result<(), ProjectionError>;

    /// Applies integrity-export supplements after all event records have folded.
    ///
    /// The default has no effect. Implementations use this only for immutable data, such as
    /// actual committed artifact dependencies, which is part of the same checked journal export
    /// but not an event frame. This receives no external-effect capability.
    ///
    /// # Errors
    ///
    /// Returns a typed order or invariant error for an invalid checked supplement.
    fn finish(
        &self,
        _state: &mut Self::State,
        _export: &IntegrityExport,
    ) -> Result<(), ProjectionError> {
        Ok(())
    }

    /// Applies one independently verified immutable command-batch supplement during suffix replay.
    ///
    /// This hook represents only projection-owned summary data. It does not restore a native
    /// application, run, provider, or operating-system context.
    ///
    /// # Errors
    ///
    /// Returns an order or invariant error when the supplement cannot extend the accepted state.
    fn fold_supplement(
        &self,
        _state: &mut Self::State,
        _export: &IntegrityExport,
    ) -> Result<(), ProjectionError> {
        Ok(())
    }
}
