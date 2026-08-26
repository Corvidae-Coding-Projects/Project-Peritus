//! Content-addressed finalization for F0 decision and activation artifacts.

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, Publication, WriteRequest,
};
use peritus_types::{EventId, Sha256Digest};

use crate::{EvolutionError, EvolutionErrorKind, EvolutionOperation, EvolutionRecovery};

/// Exact finalized artifact observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FinalizedEvolutionArtifact {
    semantic_digest: Sha256Digest,
    artifact_digest: ArtifactDigest,
    size: u64,
    publication: Publication,
}

impl FinalizedEvolutionArtifact {
    /// Domain semantic digest supplied by the checked producer.
    #[must_use]
    pub const fn semantic_digest(self) -> Sha256Digest {
        self.semantic_digest
    }
    /// SHA-256 identity of the exact artifact bytes.
    #[must_use]
    pub const fn artifact_digest(self) -> ArtifactDigest {
        self.artifact_digest
    }
    /// Exact finalized byte count.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }
    /// Whether the owner created or reused the content.
    #[must_use]
    pub const fn publication(self) -> Publication {
        self.publication
    }
}

/// Streams exact canonical F0 bytes to the artifact owner and verifies finalization.
///
/// # Errors
/// Rejects empty or oversized input and any artifact-owner staging, finalization, or verification
/// disagreement.
pub fn finalize_evolution_artifact(
    store: &ArtifactStore,
    bytes: &[u8],
    semantic_digest: Sha256Digest,
    creating_event: EventId,
) -> Result<FinalizedEvolutionArtifact, EvolutionError> {
    let size = u64::try_from(bytes.len()).map_err(|_| artifact("artifact size overflowed"))?;
    if size == 0 {
        return Err(artifact("evolution artifact is empty"));
    }
    let artifact_digest = ArtifactDigest::from_sha256(peritus_codec::sha256(bytes));
    let media_type = MediaType::new("application/vnd.peritus.evolution+binary")
        .map_err(|_| artifact("evolution artifact media type is invalid"))?;
    let request = WriteRequest::new(
        artifact_digest,
        size,
        size,
        media_type,
        EncryptionMetadata::unencrypted(),
        creating_event,
    );
    let mut writer = store.begin_write(request).map_err(artifact_owner)?;
    writer.write_chunk(bytes).map_err(artifact_owner)?;
    let finalized = writer.finalize().map_err(artifact_owner)?;
    let verified = store.verify(finalized.digest()).map_err(artifact_owner)?;
    if finalized.digest() != artifact_digest || finalized.size() != size || verified.size() != size
    {
        return Err(artifact("finalized artifact differs from exact input bytes"));
    }
    Ok(FinalizedEvolutionArtifact {
        semantic_digest,
        artifact_digest,
        size,
        publication: finalized.publication(),
    })
}

fn artifact_owner(_: impl core::fmt::Display) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Artifact,
        EvolutionOperation::Publish,
        EvolutionRecovery::Reconcile,
        "artifact owner could not finalize or verify evolution content",
    )
}

const fn artifact(detail: &'static str) -> EvolutionError {
    EvolutionError::new(
        EvolutionErrorKind::Artifact,
        EvolutionOperation::Publish,
        EvolutionRecovery::Reconcile,
        detail,
    )
}
