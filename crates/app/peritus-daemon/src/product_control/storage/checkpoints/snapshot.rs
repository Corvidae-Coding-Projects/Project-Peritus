//! Versioned immutable chunk roots; the control journal publishes only snapshot metadata.

use super::{ControlError, ControlOperation, ControlReceipt, ControlStore, Error};
use super::super::ControlGeneration;
use peritus_artifact_store::{
    ArtifactDigest, EncryptionMetadata, MediaType, ReferenceOwner, WriteRequest,
};
use peritus_journal::StateInstall;
use peritus_patch::{FileMode, SnapshotFile};
use peritus_product_runner::control::{
    CheckpointFileVersion, CheckpointId, ControlIntent, UserCheckpoint,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    fs::File,
    io::{self, Read},
    sync::Arc,
};

// A physical transfer chunk, never a limit on a file or on a checkpoint's aggregate content.
pub(in crate::product_control::storage) const CHUNK_BYTES: usize = 1024 * 1024;
const SNAPSHOT_NAMESPACE: u16 = 3482;
const RESTORE_EVIDENCE_NAMESPACE: u16 = 3483;
const NODE_MAGIC: &[u8; 8] = b"pcchunk1";
mod control;
#[allow(
    dead_code,
    reason = "the legacy private writer remains decode-compatible while new callers pre-spool"
)]
mod manifest;
pub(in crate::product_control::storage) use control::ControlPublications;
mod publication;
pub(crate) use publication::{PublicationClaim, PublicationPurpose, RetainedPublication};
pub(in crate::product_control::storage) use publication::{
    PendingPublication, PublicationCoordinator, PublicationInventory, PublicationLeaseSet,
    PublicationPlan,
};
mod reader;
pub use reader::CheckpointSnapshots;
#[cfg(test)]
mod faults;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub use faults::{SnapshotFaultPoint, inject_snapshot_fault};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChunkRoot {
    count: u64,
    last: [u8; 32],
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BodyRoots {
    schema: u16,
    bodies: Vec<Option<ChunkRoot>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceRoot {
    schema: u16,
    root: ChunkRoot,
    digest: [u8; 32],
    bytes: u64,
}

/// Fully finalized checkpoint artifacts with no marker, reference, or C0 mutation yet.
pub(crate) struct PreparedCheckpointSnapshots {
    operation: ControlOperation,
    checkpoint: UserCheckpoint,
    roots: Vec<Option<ChunkRoot>>,
    install: Vec<u8>,
    publication: PublicationPlan,
}

/// Fully finalized restore evidence with no marker, reference, or C0 mutation yet.
pub(crate) struct PreparedRestoreEvidence {
    operation: ControlOperation,
    restore: peritus_product_runner::control::RestoreId,
    expected: [u8; 32],
    install: Vec<u8>,
    publication: PublicationPlan,
}

#[derive(Serialize)]
struct PreparedPagedBodyRoots {
    schema: u16,
    entries: u64,
    manifest: EvidenceRoot,
}

#[derive(Serialize)]
struct PreparedSnapshotManifest {
    schema: u16,
    checkpoint: UserCheckpoint,
    bodies: Vec<PreparedBodyEntry>,
}

#[derive(Serialize)]
struct PreparedBodyEntry {
    path_id: [u8; 32],
    root: Option<ChunkRoot>,
}

pub(in crate::product_control::storage) fn reference_owner(
    namespace: u16,
    id: &[u8; 16],
) -> ReferenceOwner {
    if namespace == SNAPSHOT_NAMESPACE {
        return ReferenceOwner::journal(peritus_codec::sha256(id));
    }
    let mut identity = b"peritus-checkpoint-evidence-owner-v1\0".to_vec();
    identity.extend_from_slice(&namespace.to_be_bytes());
    identity.extend_from_slice(id);
    ReferenceOwner::journal(peritus_codec::sha256(&identity))
}

impl ControlGeneration {
    /// Finalizes checkpoint bytes without creating a publication marker or durable reference.
    pub(crate) fn prepare_checkpoint_snapshots(
        &self,
        operation: &ControlOperation,
        bodies: &[Option<tempfile::TempPath>],
    ) -> Result<PreparedCheckpointSnapshots, Error> {
        let checkpoint = match operation.intent() {
            ControlIntent::CreateCheckpoint(checkpoint)
            | ControlIntent::CreateAutomaticCheckpoint(checkpoint) => checkpoint,
            _ => return Err(ControlError::InvalidInput.into()),
        };
        prepare_snapshot_operation(self, operation, checkpoint, bodies)
    }

    /// Finalizes recovery checkpoint bytes without entering a C0 authority lane.
    pub(crate) fn prepare_restore_snapshots(
        &self,
        operation: &ControlOperation,
        bodies: &[Option<tempfile::TempPath>],
    ) -> Result<PreparedCheckpointSnapshots, Error> {
        let checkpoint = match operation.intent() {
            ControlIntent::PrepareRestore { recovery, .. }
            | ControlIntent::PrepareAutomaticRestore { recovery, .. } => recovery,
            _ => return Err(ControlError::InvalidInput.into()),
        };
        prepare_snapshot_operation(self, operation, checkpoint, bodies)
    }

    pub(crate) fn prepare_restore_evidence(
        &self,
        operation: &ControlOperation,
        restore: peritus_product_runner::control::RestoreId,
        expected: [u8; 32],
        bytes: &[u8],
    ) -> Result<PreparedRestoreEvidence, Error> {
        if peritus_codec::sha256(bytes).as_bytes() != &expected {
            return Err(ControlError::InvalidInput.into());
        }
        let store = artifact(peritus_artifact_store::ArtifactStore::open(
            self.reply_artifact_config().clone(),
        ))?;
        let version = CheckpointFileVersion::present(
            peritus_types::Sha256Digest::new(expected),
            bytes.len() as u64,
            CheckpointFileMode::Regular,
        );
        let (root, artifacts) = spool_stream_in(
            &store,
            operation,
            &mut io::Cursor::new(bytes),
            version,
        )?;
        let record = EvidenceRoot { schema: 1, root, digest: expected, bytes: bytes.len() as u64 };
        let install = serde_json::to_vec(&record)
            .map_err(|_| Error::Corrupt("cannot encode restore evidence root"))?;
        let claim = publication_claim(
            operation,
            RESTORE_EVIDENCE_NAMESPACE,
            *restore.as_bytes(),
            &install,
            PublicationPurpose::RestoreEvidence,
        )?;
        let publication = PublicationPlan::new(
            claim,
            reference_owner(RESTORE_EVIDENCE_NAMESPACE, restore.as_bytes()),
            artifacts,
        )?;
        Ok(PreparedRestoreEvidence {
            operation: operation.clone(),
            restore,
            expected,
            install,
            publication,
        })
    }
}

impl ControlStore {
    pub(crate) fn checkpoint_uses_chunks(&self, checkpoint: CheckpointId) -> Result<bool, Error> {
        Ok(self.journal.state_record(SNAPSHOT_NAMESPACE, checkpoint.as_bytes())?.is_some())
    }

    /// Compatibility wrapper. Production callers should prepare before acquiring authority.
    pub(crate) fn accept_checkpoint_snapshots(
        &mut self,
        operation: &ControlOperation,
        bodies: &[Option<tempfile::TempPath>],
    ) -> Result<ControlReceipt, Error> {
        let prepared = self.generation().prepare_checkpoint_snapshots(operation, bodies)?;
        self.accept_prepared_checkpoint_snapshots(prepared)
    }

    /// Compatibility wrapper. Production callers should prepare before acquiring authority.
    pub(crate) fn accept_restore_snapshots(
        &mut self,
        operation: &ControlOperation,
        bodies: &[Option<tempfile::TempPath>],
    ) -> Result<ControlReceipt, Error> {
        let prepared = self.generation().prepare_restore_snapshots(operation, bodies)?;
        self.accept_prepared_checkpoint_snapshots(prepared)
    }

    pub(crate) fn accept_prepared_checkpoint_snapshots(
        &mut self,
        prepared: PreparedCheckpointSnapshots,
    ) -> Result<ControlReceipt, Error> {
        if let Some(receipt) = self.resolve(&prepared.operation)? {
            return Ok(receipt);
        }
        self.validate_snapshot_restore(
            &prepared.operation,
            &prepared.checkpoint,
            &prepared.roots,
        )?;
        let install = StateInstall::new(
            SNAPSHOT_NAMESPACE,
            prepared.checkpoint.id().as_bytes().to_vec(),
            None,
            1,
            prepared.install,
        )?;
        let mut append = self.prepare_installs(&prepared.operation, vec![install])?;
        append.publications.add_plan(prepared.publication);
        self.commit_prepared(append).map(|committed| committed.into_receipt())
    }

    pub(super) fn accept_restore_evidence(
        &mut self,
        operation: &ControlOperation,
        restore: peritus_product_runner::control::RestoreId,
        expected: [u8; 32],
        bytes: &[u8],
    ) -> Result<ControlReceipt, Error> {
        let prepared =
            self.generation().prepare_restore_evidence(operation, restore, expected, bytes)?;
        self.accept_prepared_restore_evidence(prepared)
    }

    pub(crate) fn accept_prepared_restore_evidence(
        &mut self,
        prepared: PreparedRestoreEvidence,
    ) -> Result<ControlReceipt, Error> {
        if let Some(receipt) = self.resolve(&prepared.operation)? {
            return Ok(receipt);
        }
        if prepared.expected
            != match prepared.operation.intent() {
                ControlIntent::SettleRestore { transaction_manifest_digest: Some(expected), .. }
                | ControlIntent::SettleAutomaticRestore {
                    transaction_manifest_digest: Some(expected), ..
                } => *expected,
                _ => prepared.expected,
            }
        {
            return Err(ControlError::IdempotencyConflict.into());
        }
        let install = StateInstall::new(
            RESTORE_EVIDENCE_NAMESPACE,
            prepared.restore.as_bytes().to_vec(),
            None,
            1,
            prepared.install,
        )?;
        let mut append = self.prepare_installs(&prepared.operation, vec![install])?;
        append.publications.add_plan(prepared.publication);
        self.commit_prepared(append).map(|committed| committed.into_receipt())
    }

    pub(in crate::product_control::storage) fn spool_stream(
        &self,
        operation: &ControlOperation,
        input: &mut dyn Read,
        version: CheckpointFileVersion,
    ) -> Result<(ChunkRoot, Vec<ArtifactDigest>), Error> {
        spool_stream_in(&self.checkpoint_artifacts, operation, input, version)
    }

    // Kept for the historical manifest decoder's private writer. New paths use spool_stream.
    fn publish_stream(
        &self,
        operation: &ControlOperation,
        owner: ReferenceOwner,
        pending: &mut PendingPublication,
        input: &mut dyn Read,
        version: CheckpointFileVersion,
    ) -> Result<ChunkRoot, Error> {
        let (root, artifacts) = self.spool_stream(operation, input, version)?;
        for digest in artifacts {
            pending.record(digest)?;
            artifact(self.checkpoint_artifacts.add_reference(owner, digest))?;
        }
        Ok(root)
    }

    pub(super) fn verify_restore_evidence(
        &self,
        restore: peritus_product_runner::control::RestoreId,
        expected: &[u8; 32],
        position: u64,
    ) -> Result<bool, Error> {
        let Some(record) =
            self.journal.state_record(RESTORE_EVIDENCE_NAMESPACE, restore.as_bytes())?
        else {
            return Ok(false);
        };
        let root: EvidenceRoot = serde_json::from_slice(record.bytes())
            .map_err(|_| Error::Corrupt("restore evidence root invalid"))?;
        if root.schema != 1
            || &root.digest != expected
            || record.revision() != 1
            || record.producing_position() != position
        {
            return Err(Error::Corrupt("restore evidence root differs from its journal event"));
        }
        let version = CheckpointFileVersion::present(
            peritus_types::Sha256Digest::new(root.digest),
            root.bytes,
            peritus_product_runner::control::CheckpointFileMode::Regular,
        );
        self.verify_stream(root.root, version)?;
        Ok(true)
    }

    fn validate_snapshot_restore(
        &self,
        operation: &ControlOperation,
        checkpoint: &UserCheckpoint,
        roots: &[Option<ChunkRoot>],
    ) -> Result<(), Error> {
        use peritus_patch::{PatchOperation, PatchSet, Preimage, WorkspacePath};
        let mut operations = Vec::with_capacity(checkpoint.paths().len());
        for (path, root) in checkpoint.paths().iter().zip(roots) {
            let workspace_path =
                WorkspacePath::new(path.path()).map_err(|_| ControlError::InvalidInput)?;
            let operation = match (path.checkpoint(), root) {
                (CheckpointFileVersion::EmptyDirectory { permissions }, None) => {
                    Ok(PatchOperation::create_directory(
                        workspace_path,
                        peritus_patch::DirectoryMode::new(permissions)
                            .map_err(|_| ControlError::InvalidInput)?,
                    ))
                }
                (CheckpointFileVersion::Absent, None) => PatchOperation::delete(
                    workspace_path,
                    Preimage::from_bytes(&[], FileMode::Regular),
                ),
                (version @ CheckpointFileVersion::Present { .. }, Some(root)) => {
                    let source = Arc::new(self.snapshot_source(*root, version)?);
                    let snapshot = SnapshotFile::from_source(
                        source,
                        version.digest().ok_or(ControlError::InvalidInput)?,
                        version.bytes().ok_or(ControlError::InvalidInput)?,
                        mode(version)?,
                    );
                    PatchOperation::replace_snapshot(workspace_path, snapshot.identity(), snapshot)
                }
                _ => return Err(ControlError::InvalidInput.into()),
            }
            .map_err(|_| ControlError::InvalidInput)?;
            operations.push(operation);
        }
        if !operations.is_empty() {
            PatchSet::from_snapshot(
                peritus_types::WorkspaceId::new(*operation.workspace_bytes())
                    .map_err(|_| ControlError::InvalidInput)?,
                peritus_types::Generation::first(),
                peritus_types::RevisionNumber::first(),
                operations,
            )
            .map_err(|_| ControlError::Capacity)?;
        }
        Ok(())
    }
}

fn prepare_snapshot_operation(
    generation: &ControlGeneration,
    operation: &ControlOperation,
    checkpoint: &UserCheckpoint,
    bodies: &[Option<tempfile::TempPath>],
) -> Result<PreparedCheckpointSnapshots, Error> {
    if checkpoint.paths().len() != bodies.len() {
        return Err(ControlError::InvalidInput.into());
    }
    let store = artifact(peritus_artifact_store::ArtifactStore::open(
        generation.reply_artifact_config().clone(),
    ))?;
    let mut roots = Vec::with_capacity(bodies.len());
    let mut artifacts = Vec::new();
    for (path, body) in checkpoint.paths().iter().zip(bodies) {
        roots.push(match (path.checkpoint(), body) {
            (
                CheckpointFileVersion::Absent | CheckpointFileVersion::EmptyDirectory { .. },
                None,
            ) => None,
            (version @ CheckpointFileVersion::Present { .. }, Some(body)) => {
                let mut input = File::open(body)?;
                let (root, mut spooled) =
                    spool_stream_in(&store, operation, &mut input, version)?;
                artifacts.append(&mut spooled);
                Some(root)
            }
            _ => return Err(ControlError::InvalidInput.into()),
        });
    }
    let bodies = checkpoint
        .paths()
        .iter()
        .zip(&roots)
        .map(|(path, root)| PreparedBodyEntry {
            path_id: path.path_id().into_bytes(),
            root: *root,
        })
        .collect();
    let manifest = PreparedSnapshotManifest {
        schema: 1,
        checkpoint: checkpoint.clone(),
        bodies,
    };
    let manifest_bytes = serde_json::to_vec(&manifest)
        .map_err(|_| Error::Corrupt("cannot encode checkpoint manifest pages"))?;
    let digest = peritus_codec::sha256(&manifest_bytes);
    let size = u64::try_from(manifest_bytes.len()).map_err(|_| ControlError::Capacity)?;
    let version = CheckpointFileVersion::present(digest, size, CheckpointFileMode::Regular);
    let (manifest_root, mut manifest_artifacts) = spool_stream_in(
        &store,
        operation,
        &mut io::Cursor::new(&manifest_bytes),
        version,
    )?;
    artifacts.append(&mut manifest_artifacts);
    let install = serde_json::to_vec(&PreparedPagedBodyRoots {
        schema: 2,
        entries: u64::try_from(checkpoint.paths().len()).map_err(|_| ControlError::Capacity)?,
        manifest: EvidenceRoot {
            schema: 1,
            root: manifest_root,
            digest: digest.into_bytes(),
            bytes: size,
        },
    })
    .map_err(|_| Error::Corrupt("cannot encode checkpoint manifest root"))?;
    let claim = publication_claim(
        operation,
        SNAPSHOT_NAMESPACE,
        *checkpoint.id().as_bytes(),
        &install,
        PublicationPurpose::Snapshot,
    )?;
    let publication = PublicationPlan::new(
        claim,
        reference_owner(SNAPSHOT_NAMESPACE, checkpoint.id().as_bytes()),
        artifacts,
    )?;
    Ok(PreparedCheckpointSnapshots {
        operation: operation.clone(),
        checkpoint: checkpoint.clone(),
        roots,
        install,
        publication,
    })
}

fn spool_stream_in(
    store: &peritus_artifact_store::ArtifactStore,
    operation: &ControlOperation,
    input: &mut dyn Read,
    version: CheckpointFileVersion,
) -> Result<(ChunkRoot, Vec<ArtifactDigest>), Error> {
    let mut root = ChunkRoot { count: 0, last: [0; 32] };
    let mut artifacts = Vec::new();
    let mut chunk = vec![0_u8; CHUNK_BYTES];
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    loop {
        let mut count = 0;
        while count < chunk.len() {
            let read = input.read(&mut chunk[count..])?;
            if read == 0 {
                break;
            }
            count += read;
        }
        if count == 0 {
            break;
        }
        total = total.checked_add(count as u64).ok_or(ControlError::Capacity)?;
        if version.bytes().is_none_or(|expected| total > expected) {
            return Err(Error::Corrupt("captured checkpoint body grew before publication"));
        }
        hasher.update(&chunk[..count]);
        let digest = spool_chunk(store, operation, &chunk[..count])?;
        artifacts.push(digest);
        let mut node = NODE_MAGIC.to_vec();
        node.extend_from_slice(&root.last);
        node.extend_from_slice(digest.as_bytes());
        let node_digest = spool_chunk(store, operation, &node)?;
        artifacts.push(node_digest);
        root.last = *node_digest.as_bytes();
        root.count = root.count.checked_add(1).ok_or(ControlError::Capacity)?;
    }
    if version.bytes() != Some(total)
        || version.digest() != Some(peritus_types::Sha256Digest::new(hasher.finalize().into()))
    {
        return Err(Error::Corrupt("captured checkpoint body changed before publication"));
    }
    Ok((root, artifacts))
}

fn spool_chunk(
    store: &peritus_artifact_store::ArtifactStore,
    operation: &ControlOperation,
    bytes: &[u8],
) -> Result<ArtifactDigest, Error> {
    let digest = ArtifactDigest::from_sha256(peritus_codec::sha256(bytes));
    let existing = artifact(store.metadata(digest))?;
    if existing.as_ref().is_none_or(|metadata| !metadata.is_referenceable()) {
        let request = WriteRequest::new(
            digest,
            bytes.len() as u64,
            CHUNK_BYTES as u64,
            artifact(MediaType::new("application/octet-stream"))?,
            EncryptionMetadata::unencrypted(),
            super::super::event_id(operation)?,
        );
        let mut writer = artifact(store.begin_write(request))?;
        artifact(writer.write_chunk(bytes))?;
        #[cfg(test)]
        faults::check(SnapshotFaultPoint::BeforeChunkFinalization)?;
        artifact(writer.finalize())?;
    }
    let metadata = artifact(store.verify(digest))?;
    if metadata.digest() != digest || metadata.size() != bytes.len() as u64 {
        return Err(Error::Corrupt("spooled checkpoint chunk differs from its identity"));
    }
    Ok(digest)
}

pub(in crate::product_control::storage) fn publication_claim(
    operation: &ControlOperation,
    namespace: u16,
    id: [u8; 16],
    immutable_root: &[u8],
    purpose: PublicationPurpose,
) -> Result<PublicationClaim, Error> {
    let operation_bytes = operation.canonical_bytes()?;
    let mut binding = b"peritus-control-publication-claim-v1\0".to_vec();
    binding.extend_from_slice(&operation_bytes);
    binding.extend_from_slice(immutable_root);
    let bytes = u64::try_from(operation_bytes.len())
        .map_err(|_| ControlError::Capacity)?
        .checked_add(u64::try_from(immutable_root.len()).map_err(|_| ControlError::Capacity)?)
        .ok_or(ControlError::Capacity)?;
    PublicationClaim::new(
        namespace,
        id,
        peritus_codec::sha256(&binding).into_bytes(),
        bytes,
        purpose,
    )
}

fn artifact<T>(result: Result<T, peritus_artifact_store::ArtifactStoreError>) -> Result<T, Error> {
    result.map_err(|error| Error::Io(io::Error::other(error)))
}
fn mode(version: CheckpointFileVersion) -> Result<FileMode, Error> {
    match version.mode().ok_or(ControlError::InvalidInput)? {
        peritus_product_runner::control::CheckpointFileMode::Regular => Ok(FileMode::Regular),
        peritus_product_runner::control::CheckpointFileMode::Executable => Ok(FileMode::Executable),
    }
}
