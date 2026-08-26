//! Narrow verified artifact-content port used by the effect shell.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_types::Sha256Digest;

use crate::materialization::{
    MaterializationError, MaterializationErrorKind, MaterializationRecovery,
};

/// Exact bytes returned from an active finalized C0 artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedArtifact {
    digest: Sha256Digest,
    bytes: Vec<u8>,
}

impl VerifiedArtifact {
    fn checked(
        digest: Sha256Digest,
        maximum_bytes: u64,
        bytes: Vec<u8>,
    ) -> Result<Self, MaterializationError> {
        let size =
            u64::try_from(bytes.len()).map_err(|_| artifact("artifact length overflowed"))?;
        if size > maximum_bytes || peritus_codec::sha256(&bytes) != digest {
            return Err(artifact("artifact bytes exceed the bound or disagree with their digest"));
        }
        Ok(Self { digest, bytes })
    }

    /// Returns the verified content digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Borrows the complete verified bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Effect port that returns only active finalized, bounded, digest-verified artifact bytes.
pub trait ArtifactReader {
    /// Reads and verifies one exact content artifact.
    ///
    /// # Errors
    /// Returns a typed artifact failure for missing, inactive, oversized, or corrupt content.
    fn read_artifact(
        &self,
        digest: Sha256Digest,
        maximum_bytes: u64,
    ) -> Result<VerifiedArtifact, MaterializationError>;
}

impl ArtifactReader for ArtifactStore {
    fn read_artifact(
        &self,
        digest: Sha256Digest,
        maximum_bytes: u64,
    ) -> Result<VerifiedArtifact, MaterializationError> {
        let bytes = self
            .read(ArtifactDigest::from_sha256(digest), maximum_bytes)
            .map_err(|_| artifact("C0 could not return an active finalized artifact"))?;
        VerifiedArtifact::checked(digest, maximum_bytes, bytes)
    }
}

fn artifact(detail: &'static str) -> MaterializationError {
    MaterializationError::new(
        MaterializationErrorKind::Artifact,
        MaterializationRecovery::Reconcile,
        detail,
    )
}
