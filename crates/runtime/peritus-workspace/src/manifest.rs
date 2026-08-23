//! Canonical content-addressed workspace outcome manifests.

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, FinalizedArtifact, MediaType, WriteRequest,
};
use peritus_git::TreeId;
use peritus_types::{ActionId, EventId, Generation, RevisionNumber, Sha256Digest, WorkspaceId};

use crate::RestartObservation;

/// Stable workspace manifest family.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ManifestKind {
    /// A checked patch result was retained as a new candidate.
    Candidate,
    /// An earlier snapshot tree was restored as a new successor.
    Rollback,
    /// A complete restart or post-fence inspection was recorded.
    Reconciliation,
}

/// Complete canonical workspace outcome description.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceManifest {
    kind: ManifestKind,
    workspace_id: WorkspaceId,
    generation: Generation,
    prior_revision: RevisionNumber,
    current_revision: RevisionNumber,
    action_id: Option<ActionId>,
    action_digest: Option<Sha256Digest>,
    tree: TreeId,
    detail_digest: Sha256Digest,
    bytes: Vec<u8>,
    digest: ArtifactDigest,
}

impl WorkspaceManifest {
    /// Creates a canonical candidate manifest.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn candidate(
        workspace_id: WorkspaceId,
        generation: Generation,
        prior_revision: RevisionNumber,
        current_revision: RevisionNumber,
        action_id: ActionId,
        action_digest: Sha256Digest,
        tree: TreeId,
        detail_digest: Sha256Digest,
    ) -> Self {
        Self::mutation(
            ManifestKind::Candidate,
            workspace_id,
            generation,
            prior_revision,
            current_revision,
            action_id,
            action_digest,
            tree,
            detail_digest,
        )
    }

    /// Creates a canonical rollback manifest.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn rollback(
        workspace_id: WorkspaceId,
        generation: Generation,
        prior_revision: RevisionNumber,
        current_revision: RevisionNumber,
        action_id: ActionId,
        action_digest: Sha256Digest,
        tree: TreeId,
        detail_digest: Sha256Digest,
    ) -> Self {
        Self::mutation(
            ManifestKind::Rollback,
            workspace_id,
            generation,
            prior_revision,
            current_revision,
            action_id,
            action_digest,
            tree,
            detail_digest,
        )
    }

    /// Creates a canonical non-mutation reconciliation manifest.
    #[must_use]
    pub fn reconciliation(
        workspace_id: WorkspaceId,
        generation: Generation,
        revision: RevisionNumber,
        tree: TreeId,
        observation: RestartObservation,
    ) -> Self {
        Self::encode(
            ManifestKind::Reconciliation,
            workspace_id,
            generation,
            revision,
            revision,
            None,
            tree,
            observation.evidence().digest(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn mutation(
        kind: ManifestKind,
        workspace_id: WorkspaceId,
        generation: Generation,
        prior_revision: RevisionNumber,
        current_revision: RevisionNumber,
        action_id: ActionId,
        action_digest: Sha256Digest,
        tree: TreeId,
        detail_digest: Sha256Digest,
    ) -> Self {
        Self::encode(
            kind,
            workspace_id,
            generation,
            prior_revision,
            current_revision,
            Some((action_id, action_digest)),
            tree,
            detail_digest,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn encode(
        kind: ManifestKind,
        workspace_id: WorkspaceId,
        generation: Generation,
        prior_revision: RevisionNumber,
        current_revision: RevisionNumber,
        action: Option<(ActionId, Sha256Digest)>,
        tree: TreeId,
        detail_digest: Sha256Digest,
    ) -> Self {
        let mut bytes = b"PERITUS-WORKSPACE-MANIFEST-V1\0".to_vec();
        bytes.push(match kind {
            ManifestKind::Candidate => 1,
            ManifestKind::Rollback => 2,
            ManifestKind::Reconciliation => 3,
        });
        bytes.extend_from_slice(workspace_id.as_bytes());
        bytes.extend_from_slice(&generation.get().to_be_bytes());
        bytes.extend_from_slice(&prior_revision.get().to_be_bytes());
        bytes.extend_from_slice(&current_revision.get().to_be_bytes());
        match action {
            Some((action_id, action_digest)) => {
                bytes.push(1);
                bytes.extend_from_slice(action_id.as_bytes());
                bytes.extend_from_slice(action_digest.as_bytes());
            }
            None => bytes.push(0),
        }
        put_object(&mut bytes, tree);
        bytes.extend_from_slice(detail_digest.as_bytes());
        let digest = ArtifactDigest::from_sha256(peritus_codec::sha256(&bytes));
        Self {
            kind,
            workspace_id,
            generation,
            prior_revision,
            current_revision,
            action_id: action.map(|(action_id, _)| action_id),
            action_digest: action.map(|(_, action_digest)| action_digest),
            tree,
            detail_digest,
            bytes,
            digest,
        }
    }

    /// Returns the manifest family.
    #[must_use]
    pub const fn kind(&self) -> ManifestKind {
        self.kind
    }
    /// Returns the workspace lineage.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }
    /// Returns the fenced generation.
    #[must_use]
    pub const fn generation(&self) -> Generation {
        self.generation
    }
    /// Returns the predecessor logical revision.
    #[must_use]
    pub const fn prior_revision(&self) -> RevisionNumber {
        self.prior_revision
    }
    /// Returns the resulting logical revision.
    #[must_use]
    pub const fn current_revision(&self) -> RevisionNumber {
        self.current_revision
    }
    /// Returns the action, when this is a mutation outcome.
    #[must_use]
    pub const fn action_id(&self) -> Option<ActionId> {
        self.action_id
    }
    /// Returns the canonical action digest, when applicable.
    #[must_use]
    pub const fn action_digest(&self) -> Option<Sha256Digest> {
        self.action_digest
    }
    /// Returns the observed immutable tree.
    #[must_use]
    pub const fn tree(&self) -> TreeId {
        self.tree
    }
    /// Returns the subordinate Git/patch/reconciliation digest.
    #[must_use]
    pub const fn detail_digest(&self) -> Sha256Digest {
        self.detail_digest
    }
    /// Returns exact canonical bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Returns the expected artifact identity.
    #[must_use]
    pub const fn digest(&self) -> ArtifactDigest {
        self.digest
    }

    /// Synchronizes and atomically finalizes the exact manifest in the C0 artifact store.
    ///
    /// # Errors
    ///
    /// Returns the artifact store's typed finalization failure.
    pub fn finalize(
        &self,
        store: &ArtifactStore,
        creating_event: EventId,
    ) -> Result<FinalizedArtifact, crate::WorkspaceError> {
        let size = u64::try_from(self.bytes.len()).map_err(|_| artifact_error())?;
        let media_type = MediaType::new("application/vnd.peritus.workspace-manifest.v1")
            .map_err(|_| artifact_error())?;
        let request = WriteRequest::new(
            self.digest,
            size,
            size,
            media_type,
            EncryptionMetadata::unencrypted(),
            creating_event,
        );
        let mut writer = store.begin_write(request).map_err(|_| artifact_error())?;
        writer.write_chunk(&self.bytes).map_err(|_| artifact_error())?;
        writer.finalize().map_err(|_| artifact_error())
    }
}

const fn artifact_error() -> crate::WorkspaceError {
    crate::WorkspaceError::new(
        crate::ErrorCode::Artifact,
        crate::WorkspaceOperation::FinalizeManifest,
        crate::RecoveryClass::Reconcile,
        "workspace manifest could not be finalized exactly",
    )
}

fn put_object(bytes: &mut Vec<u8>, tree: TreeId) {
    let object = tree.object_id();
    bytes.push(match object.format() {
        peritus_git::ObjectFormat::Sha1 => 1,
        peritus_git::ObjectFormat::Sha256 => 2,
    });
    bytes.extend_from_slice(object.as_bytes());
}
