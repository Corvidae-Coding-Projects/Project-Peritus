//! Retained-snapshot manifest representation and restart reopening.

use peritus_types::{Sha256Digest, SnapshotId, WorkspaceId};

use super::{SCHEMA_VERSION, digest, encoded, finish, format_from_tag, format_tag};
use crate::{CommitId, GitError, SnapshotRef, TreeId};

const MAGIC: &str = "peritus-git-candidate-snapshot";

/// Durable schema-v1 representation of a retained candidate snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CandidateSnapshotManifest {
    bytes: Vec<u8>,
    digest: Sha256Digest,
    pub(super) repository_digest: Sha256Digest,
    pub(super) workspace_id: WorkspaceId,
    pub(super) snapshot_id: SnapshotId,
    pub(super) parent: CommitId,
    pub(super) commit: CommitId,
    pub(super) tree: TreeId,
    pub(super) reference: SnapshotRef,
    pub(super) candidate_digest: Sha256Digest,
}

impl CandidateSnapshotManifest {
    #[allow(clippy::too_many_arguments)] // The manifest binds these independent persisted facts.
    pub(super) fn new(
        repository_digest: Sha256Digest,
        workspace_id: WorkspaceId,
        snapshot_id: SnapshotId,
        parent: CommitId,
        commit: CommitId,
        tree: TreeId,
        reference: SnapshotRef,
        candidate_digest: Sha256Digest,
    ) -> Result<Self, GitError> {
        let mut writer = super::writer();
        let result = (|| {
            writer.write_str(MAGIC)?;
            writer.write_u16(SCHEMA_VERSION)?;
            writer.write_fixed(repository_digest.as_bytes())?;
            writer.write_fixed(workspace_id.as_bytes())?;
            writer.write_fixed(snapshot_id.as_bytes())?;
            writer.write_u8(format_tag(commit.object_id().format()))?;
            super::write_object(&mut writer, parent.object_id())?;
            super::write_object(&mut writer, commit.object_id())?;
            super::write_object(&mut writer, tree.object_id())?;
            writer.write_str(reference.as_str())?;
            writer.write_fixed(candidate_digest.as_bytes())
        })();
        let bytes = encoded(result, writer)?;
        Ok(Self {
            digest: digest(&bytes),
            bytes,
            repository_digest,
            workspace_id,
            snapshot_id,
            parent,
            commit,
            tree,
            reference,
            candidate_digest,
        })
    }

    /// Decodes one complete schema-v1 retained-snapshot manifest.
    ///
    /// # Errors
    ///
    /// Rejects unknown schemas, malformed fields, oversized input, and trailing bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, GitError> {
        let mut reader = super::reader(bytes)?;
        if reader.read_str().map_err(|_| super::invalid_manifest())? != MAGIC
            || reader.read_u16().map_err(|_| super::invalid_manifest())? != SCHEMA_VERSION
        {
            return Err(super::invalid_manifest());
        }
        let repository_digest =
            Sha256Digest::new(reader.read_fixed::<32>().map_err(|_| super::invalid_manifest())?);
        let workspace_id =
            WorkspaceId::new(reader.read_fixed::<16>().map_err(|_| super::invalid_manifest())?)
                .map_err(|_| super::invalid_manifest())?;
        let snapshot_id =
            SnapshotId::new(reader.read_fixed::<16>().map_err(|_| super::invalid_manifest())?)
                .map_err(|_| super::invalid_manifest())?;
        let format = format_from_tag(reader.read_u8().map_err(|_| super::invalid_manifest())?)
            .ok_or_else(super::invalid_manifest)?;
        let parent = CommitId::checked(super::read_object(&mut reader, format)?);
        let commit = CommitId::checked(super::read_object(&mut reader, format)?);
        let tree = TreeId::checked(super::read_object(&mut reader, format)?);
        let encoded_reference =
            reader.read_str().map_err(|_| super::invalid_manifest())?.to_owned();
        let reference = crate::snapshot::expected_snapshot_ref(workspace_id, snapshot_id);
        if encoded_reference != reference.as_str() {
            return Err(super::invalid_manifest());
        }
        let candidate_digest =
            Sha256Digest::new(reader.read_fixed::<32>().map_err(|_| super::invalid_manifest())?);
        finish(reader)?;
        Ok(Self {
            bytes: bytes.to_vec(),
            digest: digest(bytes),
            repository_digest,
            workspace_id,
            snapshot_id,
            parent,
            commit,
            tree,
            reference,
            candidate_digest,
        })
    }

    /// Returns exact canonical schema-v1 bytes suitable for durable storage.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the SHA-256 digest of the complete manifest bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Returns the repository identity stored by this manifest.
    #[must_use]
    pub const fn repository_digest(&self) -> Sha256Digest {
        self.repository_digest
    }

    /// Returns the workspace lineage stored by this manifest.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    /// Returns the snapshot identity stored by this manifest.
    #[must_use]
    pub const fn snapshot_id(&self) -> SnapshotId {
        self.snapshot_id
    }

    /// Returns the exact parent commit stored by this manifest.
    #[must_use]
    pub const fn parent(&self) -> CommitId {
        self.parent
    }

    /// Returns the retained synthetic commit stored by this manifest.
    #[must_use]
    pub const fn commit(&self) -> CommitId {
        self.commit
    }

    /// Returns the candidate tree stored by this manifest.
    #[must_use]
    pub const fn tree(&self) -> TreeId {
        self.tree
    }

    /// Returns the exact retained reference stored by this manifest.
    #[must_use]
    pub const fn reference(&self) -> &SnapshotRef {
        &self.reference
    }

    /// Returns the source candidate-manifest digest.
    #[must_use]
    pub const fn candidate_digest(&self) -> Sha256Digest {
        self.candidate_digest
    }
}
