//! Actual committed artifact-dependency catalog projection.

use crate::encoding::{put_digest, put_u64};
use crate::lifecycle::{invariant, schema};
use crate::{
    FoldContext, Projection, ProjectionError, ProjectionErrorKind, ProjectionSchema,
    ProjectionState, RecoveryClass, ReplayOutput, replay_from_genesis,
};
use peritus_codec::sha256;
use peritus_journal::IntegrityExport;
use peritus_types::Sha256Digest;
use std::collections::{BTreeMap, BTreeSet};

/// Usage summary for one actual finalized artifact digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactReferenceEntry {
    first_position: u64,
    last_position: u64,
    owners: BTreeSet<Sha256Digest>,
}

impl ArtifactReferenceEntry {
    /// Returns the first owning journal batch position.
    #[must_use]
    pub const fn first_position(&self) -> u64 {
        self.first_position
    }
    /// Returns the final owning journal batch position.
    #[must_use]
    pub const fn last_position(&self) -> u64 {
        self.last_position
    }
    /// Returns the number of distinct committed batch owners.
    #[must_use]
    pub fn owner_count(&self) -> usize {
        self.owners.len()
    }
}

/// Deterministic actual artifact-reference state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ArtifactReferenceState {
    references: BTreeMap<Sha256Digest, ArtifactReferenceEntry>,
}

impl ArtifactReferenceState {
    /// Returns the number of distinct finalized artifact digests referenced.
    #[must_use]
    pub fn len(&self) -> usize {
        self.references.len()
    }
    /// Returns whether no artifact dependency was committed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.references.is_empty()
    }
    /// Looks up one actual artifact digest.
    #[must_use]
    pub fn get(&self, digest: Sha256Digest) -> Option<&ArtifactReferenceEntry> {
        self.references.get(&digest)
    }
}

impl ProjectionState for ArtifactReferenceState {
    fn encode(&self) -> Vec<u8> {
        let mut bytes = b"peritus-artifact-references-v1\0".to_vec();
        put_u64(&mut bytes, self.references.len() as u64);
        for (digest, entry) in &self.references {
            put_digest(&mut bytes, *digest);
            put_u64(&mut bytes, entry.first_position);
            put_u64(&mut bytes, entry.last_position);
            put_u64(&mut bytes, entry.owners.len() as u64);
            for owner in &entry.owners {
                put_digest(&mut bytes, *owner);
            }
        }
        bytes
    }

    fn validate(&self) -> Result<(), ProjectionError> {
        if self.references.values().any(|entry| {
            entry.first_position == 0
                || entry.last_position < entry.first_position
                || entry.owners.is_empty()
        }) {
            return Err(invariant("invalid actual artifact reference range"));
        }
        Ok(())
    }

    fn invariant_digest(&self) -> Sha256Digest {
        let mut bytes = b"peritus-artifact-reference-invariants-v1\0".to_vec();
        bytes.extend_from_slice(&self.encode());
        sha256(&bytes)
    }
}

/// Version-one projection of actual committed artifact dependencies.
#[derive(Clone, Debug)]
pub struct ArtifactReferenceProjection {
    schema: ProjectionSchema,
}

impl ArtifactReferenceProjection {
    /// Creates the frozen actual-reference schema.
    ///
    /// # Errors
    ///
    /// Returns an identity error only if built-in constants are invalid.
    pub fn new() -> Result<Self, ProjectionError> {
        schema("artifact-references", b"actual-artifact-digest:v1;batch-owner;first-last")
            .map(|schema| Self { schema })
    }
}

impl Projection for ArtifactReferenceProjection {
    type State = ArtifactReferenceState;
    fn schema(&self) -> &ProjectionSchema {
        &self.schema
    }
    fn genesis(&self) -> Self::State {
        ArtifactReferenceState::default()
    }
    fn fold(
        &self,
        _state: &mut Self::State,
        _input: FoldContext<'_>,
    ) -> Result<(), ProjectionError> {
        Ok(())
    }

    fn finish(
        &self,
        state: &mut Self::State,
        export: &IntegrityExport,
    ) -> Result<(), ProjectionError> {
        let mut previous = None;
        for reference in export.artifact_references() {
            let order = (reference.first_position(), reference.artifact_digest());
            if previous.is_some_and(|prior| prior >= order)
                || reference.first_position() == 0
                || reference.first_position() > reference.last_position()
                || reference.last_position() > export.report().last_position()
            {
                return Err(ProjectionError::new(
                    ProjectionErrorKind::RecordOrder,
                    RecoveryClass::RepairJournal,
                    "replay artifact references",
                    "artifact references are not canonical or within the journal range",
                ));
            }
            previous = Some(order);
            let entry = state.references.entry(reference.artifact_digest()).or_insert_with(|| {
                ArtifactReferenceEntry {
                    first_position: reference.first_position(),
                    last_position: reference.last_position(),
                    owners: BTreeSet::new(),
                }
            });
            entry.first_position = entry.first_position.min(reference.first_position());
            entry.last_position = entry.last_position.max(reference.last_position());
            if !entry.owners.insert(reference.batch_hash()) {
                return Err(invariant("duplicate artifact batch ownership"));
            }
        }
        Ok(())
    }
}

/// Dedicated pure replay for actual references from the checked journal export.
///
/// # Errors
///
/// Returns the same checked replay, canonical-order, and invariant errors as generic replay.
pub fn replay_artifact_references(
    projection: &ArtifactReferenceProjection,
    export: &IntegrityExport,
) -> Result<ReplayOutput<ArtifactReferenceState>, ProjectionError> {
    replay_from_genesis(projection, export)
}
