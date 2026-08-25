//! Immutable exact candidate, workspace, artifact, and producer binding.

use peritus_types::{ActorId, ArtifactId, RevisionTuple, Sha256Digest, SnapshotId};
use sha2::{Digest, Sha256};

use crate::{
    OrchestratorError, OrchestratorErrorKind, OrchestratorLimits, OrchestratorRecoveryAction,
};

/// Complete material candidate identity; no field can advance independently.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateBinding {
    revision: RevisionTuple,
    snapshot_id: SnapshotId,
    candidate_digest: Sha256Digest,
    tree_digest: Sha256Digest,
    quality_snapshot_digest: Sha256Digest,
    artifact_id: Option<ArtifactId>,
    artifact_digest: Option<Sha256Digest>,
    producer_actors: Vec<ActorId>,
    producer_ancestries: Vec<Sha256Digest>,
    digest: Sha256Digest,
}

impl CandidateBinding {
    /// Creates one canonical material candidate binding.
    ///
    /// # Errors
    /// Rejects unpaired artifact identity/digest, empty or noncanonical producers, mismatched
    /// producer ancestry, or configured retention excess.
    #[allow(clippy::too_many_arguments, reason = "material candidate identity remains explicit")]
    pub fn new(
        revision: RevisionTuple,
        snapshot_id: SnapshotId,
        candidate_digest: Sha256Digest,
        tree_digest: Sha256Digest,
        quality_snapshot_digest: Sha256Digest,
        artifact_id: Option<ArtifactId>,
        artifact_digest: Option<Sha256Digest>,
        producer_actors: Vec<ActorId>,
        producer_ancestries: Vec<Sha256Digest>,
        limits: OrchestratorLimits,
    ) -> Result<Self, OrchestratorError> {
        let mut value = Self::from_wire(
            revision,
            snapshot_id,
            candidate_digest,
            tree_digest,
            quality_snapshot_digest,
            artifact_id,
            artifact_digest,
            producer_actors,
            producer_ancestries,
            Sha256Digest::new([0; 32]),
        );
        value.validate_shape(limits)?;
        value.digest = candidate_binding_digest(&value)?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments, reason = "exact closed-wire candidate reconstruction")]
    pub(crate) const fn from_wire(
        revision: RevisionTuple,
        snapshot_id: SnapshotId,
        candidate_digest: Sha256Digest,
        tree_digest: Sha256Digest,
        quality_snapshot_digest: Sha256Digest,
        artifact_id: Option<ArtifactId>,
        artifact_digest: Option<Sha256Digest>,
        producer_actors: Vec<ActorId>,
        producer_ancestries: Vec<Sha256Digest>,
        digest: Sha256Digest,
    ) -> Self {
        Self {
            revision,
            snapshot_id,
            candidate_digest,
            tree_digest,
            quality_snapshot_digest,
            artifact_id,
            artifact_digest,
            producer_actors,
            producer_ancestries,
            digest,
        }
    }

    pub(crate) fn validate(&self, limits: OrchestratorLimits) -> Result<(), OrchestratorError> {
        self.validate_shape(limits)?;
        if self.digest != candidate_binding_digest(self)? {
            return Err(reject(
                OrchestratorErrorKind::BindingMismatch,
                "candidate binding digest differs from its canonical fields",
            ));
        }
        Ok(())
    }

    fn validate_shape(&self, limits: OrchestratorLimits) -> Result<(), OrchestratorError> {
        let artifact_pair = self.artifact_id.is_some() == self.artifact_digest.is_some();
        let material_nonzero =
            [self.candidate_digest, self.tree_digest, self.quality_snapshot_digest]
                .iter()
                .all(|digest| digest.as_bytes().iter().any(|byte| *byte != 0))
                && self
                    .artifact_digest
                    .is_none_or(|digest| digest.as_bytes().iter().any(|byte| *byte != 0));
        let producers_canonical = !self.producer_actors.is_empty()
            && self.producer_actors.len() == self.producer_ancestries.len()
            && self.producer_actors.len() <= usize::from(limits.artifact_references())
            && strictly_ordered(&self.producer_actors)
            && self
                .producer_ancestries
                .iter()
                .all(|digest| digest.as_bytes().iter().any(|byte| *byte != 0));
        if !artifact_pair || !material_nonzero || !producers_canonical {
            return Err(reject(
                OrchestratorErrorKind::NonCanonical,
                "candidate artifact or producer binding is incomplete or noncanonical",
            ));
        }
        Ok(())
    }

    /// Returns the exact revision tuple.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
    /// Returns the immutable workspace snapshot.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }
    /// Returns the complete candidate digest.
    #[must_use]
    pub const fn candidate_digest(&self) -> Sha256Digest {
        self.candidate_digest
    }
    /// Returns the exact repository tree digest.
    #[must_use]
    pub const fn tree_digest(&self) -> Sha256Digest {
        self.tree_digest
    }
    /// Returns the exact C4 clean-quality snapshot digest supplied to D1.
    #[must_use]
    pub const fn quality_snapshot_digest(&self) -> Sha256Digest {
        self.quality_snapshot_digest
    }
    /// Returns the optional content-addressed artifact identity.
    #[must_use]
    pub const fn artifact_id(&self) -> Option<ArtifactId> {
        self.artifact_id
    }
    /// Returns the paired optional artifact digest.
    #[must_use]
    pub const fn artifact_digest(&self) -> Option<Sha256Digest> {
        self.artifact_digest
    }
    /// Borrows canonical producer actors.
    #[must_use]
    pub fn producer_actors(&self) -> &[ActorId] {
        &self.producer_actors
    }
    /// Borrows canonical producer ancestry digests.
    #[must_use]
    pub fn producer_ancestries(&self) -> &[Sha256Digest] {
        &self.producer_ancestries
    }
    /// Returns the canonical complete binding digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Returns whether every material field is exactly equal.
    #[must_use]
    pub fn materially_equal(&self, other: &Self) -> bool {
        self.revision == other.revision
            && self.snapshot_id == other.snapshot_id
            && self.candidate_digest == other.candidate_digest
            && self.tree_digest == other.tree_digest
            && self.quality_snapshot_digest == other.quality_snapshot_digest
            && self.artifact_id == other.artifact_id
            && self.artifact_digest == other.artifact_digest
            && self.producer_actors == other.producer_actors
            && self.producer_ancestries == other.producer_ancestries
            && self.digest == other.digest
    }

    /// Returns whether a claimed revision reuses the same workspace/candidate/artifact tuple.
    #[must_use]
    pub fn reuses_material(&self, other: &Self) -> bool {
        self.snapshot_id == other.snapshot_id
            && self.candidate_digest == other.candidate_digest
            && self.tree_digest == other.tree_digest
            && self.quality_snapshot_digest == other.quality_snapshot_digest
            && self.artifact_id == other.artifact_id
            && self.artifact_digest == other.artifact_digest
    }
}

fn candidate_binding_digest(value: &CandidateBinding) -> Result<Sha256Digest, OrchestratorError> {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus.orchestrator.candidate.v1\0");
    hash_revision(&mut hasher, value.revision);
    hasher.update(value.snapshot_id.as_bytes());
    hasher.update(value.candidate_digest.as_bytes());
    hasher.update(value.tree_digest.as_bytes());
    hasher.update(value.quality_snapshot_digest.as_bytes());
    hash_optional_id(&mut hasher, value.artifact_id);
    hash_optional_digest(&mut hasher, value.artifact_digest);
    let actor_count = u16::try_from(value.producer_actors.len()).map_err(|_| {
        reject(OrchestratorErrorKind::LimitExceeded, "candidate producer count is unrepresentable")
    })?;
    hasher.update(actor_count.to_be_bytes());
    for actor in &value.producer_actors {
        hasher.update(actor.as_bytes());
    }
    let ancestry_count = u16::try_from(value.producer_ancestries.len()).map_err(|_| {
        reject(OrchestratorErrorKind::LimitExceeded, "candidate ancestry count is unrepresentable")
    })?;
    hasher.update(ancestry_count.to_be_bytes());
    for ancestry in &value.producer_ancestries {
        hasher.update(ancestry.as_bytes());
    }
    Ok(Sha256Digest::new(hasher.finalize().into()))
}

fn hash_revision(hasher: &mut Sha256, revision: RevisionTuple) {
    hasher.update(revision.acceptance_spec_id().as_bytes());
    hasher.update(revision.harness_id().as_bytes());
    hasher.update(revision.workspace_id().as_bytes());
    hasher.update(revision.workspace_generation().get().to_be_bytes());
    hasher.update(revision.workspace_revision().get().to_be_bytes());
    hasher.update(revision.policy_id().as_bytes());
    hasher.update(revision.provider_profile_id().as_bytes());
}

fn hash_optional_id(hasher: &mut Sha256, value: Option<ArtifactId>) {
    hasher.update([u8::from(value.is_some())]);
    if let Some(value) = value {
        hasher.update(value.as_bytes());
    }
}

fn hash_optional_digest(hasher: &mut Sha256, value: Option<Sha256Digest>) {
    hasher.update([u8::from(value.is_some())]);
    if let Some(value) = value {
        hasher.update(value.as_bytes());
    }
}

fn strictly_ordered<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

const fn reject(kind: OrchestratorErrorKind, detail: &'static str) -> OrchestratorError {
    OrchestratorError::new(kind, OrchestratorRecoveryAction::CorrectInput, detail)
}
