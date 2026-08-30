//! Reproducibility witnesses from independent builders.

use serde::Serialize;

use crate::{
    ArtifactError, ArtifactErrorCode, ArtifactInventory, BoundedId, ReleaseBinding, ReleasePath,
    Sha256Digest, digest_bytes,
};

/// Content observation produced by one identified builder.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BuildWitness {
    builder_id: BoundedId,
    binding: ReleaseBinding,
    inventory_digest: Sha256Digest,
    artifacts: Vec<BuildArtifactWitness>,
}

impl BuildWitness {
    /// Creates a witness from an already validated artifact inventory.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] if inventory serialization fails.
    pub fn from_inventory(
        builder_id: BoundedId,
        inventory: &ArtifactInventory,
    ) -> Result<Self, ArtifactError> {
        let artifacts = inventory
            .artifacts()
            .iter()
            .map(|artifact| BuildArtifactWitness {
                path: artifact.path().clone(),
                byte_length: artifact.byte_length(),
                sha256: artifact.sha256(),
            })
            .collect();
        Ok(Self {
            builder_id,
            binding: inventory.binding().clone(),
            inventory_digest: inventory.digest()?,
            artifacts,
        })
    }

    /// Returns the builder identity.
    #[must_use]
    pub const fn builder_id(&self) -> &BoundedId {
        &self.builder_id
    }

    /// Returns the exact release binding.
    #[must_use]
    pub const fn binding(&self) -> &ReleaseBinding {
        &self.binding
    }
}

/// Stable kind of reproducibility disagreement.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReproducibilityDifferenceKind {
    /// An artifact exists only in the first build.
    MissingFromSecond,
    /// An artifact exists only in the second build.
    MissingFromFirst,
    /// The same path has different bytes or length.
    ContentMismatch,
}

/// One path-local difference between independent build outputs.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReproducibilityDifference {
    path: ReleasePath,
    kind: ReproducibilityDifferenceKind,
    first_digest: Option<Sha256Digest>,
    second_digest: Option<Sha256Digest>,
}

impl ReproducibilityDifference {
    /// Returns the differing artifact path.
    #[must_use]
    pub const fn path(&self) -> &ReleasePath {
        &self.path
    }

    /// Returns the stable difference kind.
    #[must_use]
    pub const fn kind(&self) -> ReproducibilityDifferenceKind {
        self.kind
    }
}

/// Deterministic comparison between two distinct builder witnesses.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ReproducibilityComparison {
    binding: ReleaseBinding,
    first_builder: BoundedId,
    second_builder: BoundedId,
    first_inventory_digest: Sha256Digest,
    second_inventory_digest: Sha256Digest,
    differences: Vec<ReproducibilityDifference>,
}

impl ReproducibilityComparison {
    /// Returns whether the independent builders emitted identical paths and bytes.
    #[must_use]
    pub const fn is_reproducible(&self) -> bool {
        self.differences.is_empty()
    }

    /// Returns differences in canonical path order.
    #[must_use]
    pub fn differences(&self) -> &[ReproducibilityDifference] {
        &self.differences
    }

    /// Returns the compared candidate binding.
    #[must_use]
    pub const fn binding(&self) -> &ReleaseBinding {
        &self.binding
    }

    /// Returns a deterministic digest of the complete comparison.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] if serialization fails.
    pub fn digest(&self) -> Result<Sha256Digest, ArtifactError> {
        self.canonical_json().map(|bytes| digest_bytes(&bytes))
    }

    /// Serializes deterministic compact comparison JSON.
    ///
    /// # Errors
    ///
    /// Returns [`ArtifactError`] if serialization fails.
    pub fn canonical_json(&self) -> Result<Vec<u8>, ArtifactError> {
        serde_json::to_vec(self).map_err(|source| {
            ArtifactError::serialization("serialize reproducibility comparison", source)
        })
    }
}

/// Compares artifact paths, sizes, and hashes from two independent builders.
///
/// # Errors
///
/// Returns [`ArtifactError`] if the witnesses reuse a builder identity or bind different
/// candidates. Content differences are returned as evidence rather than hidden as an error.
pub fn compare_builds(
    first: &BuildWitness,
    second: &BuildWitness,
) -> Result<ReproducibilityComparison, ArtifactError> {
    if first.builder_id == second.builder_id {
        return Err(ArtifactError::new(
            ArtifactErrorCode::Reproducibility,
            "compare independent builds",
            "reproducibility witnesses must come from distinct builders",
        ));
    }
    if first.binding != second.binding {
        return Err(ArtifactError::new(
            ArtifactErrorCode::Integrity,
            "compare independent builds",
            "builder witnesses bind different release candidates",
        ));
    }
    let mut differences = Vec::new();
    let (mut left, mut right) = (0, 0);
    while left < first.artifacts.len() || right < second.artifacts.len() {
        match (first.artifacts.get(left), second.artifacts.get(right)) {
            (Some(first_artifact), Some(second_artifact)) => {
                match first_artifact.path.cmp(&second_artifact.path) {
                    std::cmp::Ordering::Less => {
                        differences.push(ReproducibilityDifference {
                            path: first_artifact.path.clone(),
                            kind: ReproducibilityDifferenceKind::MissingFromSecond,
                            first_digest: Some(first_artifact.sha256),
                            second_digest: None,
                        });
                        left += 1;
                    }
                    std::cmp::Ordering::Greater => {
                        differences.push(ReproducibilityDifference {
                            path: second_artifact.path.clone(),
                            kind: ReproducibilityDifferenceKind::MissingFromFirst,
                            first_digest: None,
                            second_digest: Some(second_artifact.sha256),
                        });
                        right += 1;
                    }
                    std::cmp::Ordering::Equal => {
                        if first_artifact.byte_length != second_artifact.byte_length
                            || first_artifact.sha256 != second_artifact.sha256
                        {
                            differences.push(ReproducibilityDifference {
                                path: first_artifact.path.clone(),
                                kind: ReproducibilityDifferenceKind::ContentMismatch,
                                first_digest: Some(first_artifact.sha256),
                                second_digest: Some(second_artifact.sha256),
                            });
                        }
                        left += 1;
                        right += 1;
                    }
                }
            }
            (Some(first_artifact), None) => {
                differences.push(ReproducibilityDifference {
                    path: first_artifact.path.clone(),
                    kind: ReproducibilityDifferenceKind::MissingFromSecond,
                    first_digest: Some(first_artifact.sha256),
                    second_digest: None,
                });
                left += 1;
            }
            (None, Some(second_artifact)) => {
                differences.push(ReproducibilityDifference {
                    path: second_artifact.path.clone(),
                    kind: ReproducibilityDifferenceKind::MissingFromFirst,
                    first_digest: None,
                    second_digest: Some(second_artifact.sha256),
                });
                right += 1;
            }
            (None, None) => break,
        }
    }
    Ok(ReproducibilityComparison {
        binding: first.binding.clone(),
        first_builder: first.builder_id.clone(),
        second_builder: second.builder_id.clone(),
        first_inventory_digest: first.inventory_digest,
        second_inventory_digest: second.inventory_digest,
        differences,
    })
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
struct BuildArtifactWitness {
    path: ReleasePath,
    byte_length: u64,
    sha256: Sha256Digest,
}
