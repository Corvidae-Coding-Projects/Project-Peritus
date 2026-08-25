//! Closed join policy and exact artifact handoff evidence.

use peritus_types::{ArtifactId, RevisionTuple, Sha256Digest};

use crate::error::{CollaborationError, CollaborationErrorKind, reject};

/// Closed child-join semantics retained on every task.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum JoinPolicy {
    /// The task declares no required child join.
    NoChildren,
    /// Every declared required child must succeed.
    AllRequired,
    /// At least one declared required child must succeed.
    AnyRequired,
}

/// Exact-revision artifact and evidence handoff retained at task completion.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ArtifactHandoff {
    artifact_id: ArtifactId,
    artifact_digest: Sha256Digest,
    evidence_digest: Sha256Digest,
    revision: RevisionTuple,
}

impl ArtifactHandoff {
    /// Creates an exact artifact/evidence handoff.
    ///
    /// # Errors
    /// Rejects reserved all-zero artifact or evidence digests.
    pub fn new(
        artifact_id: ArtifactId,
        artifact_digest: Sha256Digest,
        evidence_digest: Sha256Digest,
        revision: RevisionTuple,
    ) -> Result<Self, CollaborationError> {
        if artifact_digest == Sha256Digest::new([0; 32])
            || evidence_digest == Sha256Digest::new([0; 32])
        {
            return Err(reject(
                CollaborationErrorKind::InvalidInput,
                "artifact and evidence handoff digests must be nonzero",
            ));
        }
        Ok(Self { artifact_id, artifact_digest, evidence_digest, revision })
    }

    /// Returns the durable artifact identity.
    #[must_use]
    pub const fn artifact_id(self) -> ArtifactId {
        self.artifact_id
    }
    /// Returns the exact artifact digest.
    #[must_use]
    pub const fn artifact_digest(self) -> Sha256Digest {
        self.artifact_digest
    }
    /// Returns the exact handoff-evidence digest.
    #[must_use]
    pub const fn evidence_digest(self) -> Sha256Digest {
        self.evidence_digest
    }
    /// Returns the exact revision.
    #[must_use]
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }
}
