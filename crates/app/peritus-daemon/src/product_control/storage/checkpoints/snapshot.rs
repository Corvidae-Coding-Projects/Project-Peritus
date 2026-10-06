//! Versioned immutable chunk roots; the control journal publishes only snapshot metadata.

use super::{ControlError, ControlOperation, ControlReceipt, ControlStore, Error};
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
mod publication;
mod reader;
use publication::PendingPublication;
#[cfg(test)]
mod faults;
#[cfg(test)]
pub use faults::{SnapshotFaultPoint, inject_snapshot_fault};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
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

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EvidenceRoot {
    schema: u16,
    root: ChunkRoot,
    digest: [u8; 32],
    bytes: u64,
}

fn reference_owner(namespace: u16, id: &[u8; 16]) -> ReferenceOwner {
    if namespace == SNAPSHOT_NAMESPACE {
        return ReferenceOwner::journal(peritus_codec::sha256(id));
    }
    let mut identity = b"peritus-checkpoint-evidence-owner-v1\0".to_vec();
    identity.extend_from_slice(&namespace.to_be_bytes());
    identity.extend_from_slice(id);
    ReferenceOwner::journal(peritus_codec::sha256(&identity))
}

impl ControlStore {
    pub(crate) fn checkpoint_uses_chunks(&self, checkpoint: CheckpointId) -> Result<bool, Error> {
        Ok(self.journal.state_record(SNAPSHOT_NAMESPACE, checkpoint.as_bytes())?.is_some())
    }
    pub(crate) fn accept_checkpoint_snapshots(
        &mut self,
        operation: &ControlOperation,
        bodies: &[Option<tempfile::TempPath>],
    ) -> Result<ControlReceipt, Error> {
        let checkpoint = match operation.intent() {
            ControlIntent::CreateCheckpoint(checkpoint)
            | ControlIntent::CreateAutomaticCheckpoint(checkpoint) => checkpoint,
            _ => return Err(ControlError::InvalidInput.into()),
        };
        self.accept_snapshot_operation(operation, checkpoint, bodies)
    }

    pub(crate) fn accept_restore_snapshots(
        &mut self,
        operation: &ControlOperation,
        bodies: &[Option<tempfile::TempPath>],
    ) -> Result<ControlReceipt, Error> {
        let checkpoint = match operation.intent() {
            ControlIntent::PrepareRestore { recovery, .. }
            | ControlIntent::PrepareAutomaticRestore { recovery, .. } => recovery,
            _ => return Err(ControlError::InvalidInput.into()),
        };
        self.accept_snapshot_operation(operation, checkpoint, bodies)
    }

    fn accept_snapshot_operation(
        &mut self,
        operation: &ControlOperation,
        checkpoint: &UserCheckpoint,
        bodies: &[Option<tempfile::TempPath>],
    ) -> Result<ControlReceipt, Error> {
        if let Some(receipt) = self.resolve(operation)? {
            return Ok(receipt);
        }
        if checkpoint.paths().len() != bodies.len() {
            return Err(ControlError::InvalidInput.into());
        }
        self.recover_snapshot_publications()?;
        let owner = reference_owner(SNAPSHOT_NAMESPACE, checkpoint.id().as_bytes());
        let mut pending = PendingPublication::new(
            self.checkpoint_config.root(),
            SNAPSHOT_NAMESPACE,
            checkpoint.id().as_bytes(),
        )?;
        let mut roots = Vec::with_capacity(bodies.len());
        for (path, body) in checkpoint.paths().iter().zip(bodies) {
            roots.push(match (path.checkpoint(), body) {
                (
                    CheckpointFileVersion::Absent | CheckpointFileVersion::EmptyDirectory { .. },
                    None,
                ) => None,
                (version @ CheckpointFileVersion::Present { .. }, Some(body)) => {
                    Some(self.publish_snapshot(operation, owner, &mut pending, body, version)?)
                }
                _ => return Err(ControlError::InvalidInput.into()),
            });
        }
        self.validate_snapshot_restore(operation, checkpoint, &roots)?;
        let bytes = serde_json::to_vec(&BodyRoots { schema: 1, bodies: roots })
            .map_err(|_| Error::Corrupt("cannot encode checkpoint chunk roots"))?;
        #[cfg(test)]
        faults::check(SnapshotFaultPoint::BeforeRootPublication)?;
        let receipt = self.accept_installs(
            operation,
            vec![StateInstall::new(
                SNAPSHOT_NAMESPACE,
                checkpoint.id().as_bytes().to_vec(),
                None,
                1,
                bytes,
            )?],
        )?;
        #[cfg(test)]
        faults::check(SnapshotFaultPoint::AfterRootPublication)?;
        pending.finish()?;
        Ok(receipt)
    }

    fn publish_snapshot(
        &self,
        operation: &ControlOperation,
        owner: ReferenceOwner,
        pending: &mut PendingPublication,
        body: &tempfile::TempPath,
        version: CheckpointFileVersion,
    ) -> Result<ChunkRoot, Error> {
        let mut input = File::open(body)?;
        self.publish_stream(operation, owner, pending, &mut input, version)
    }

    fn publish_stream(
        &self,
        operation: &ControlOperation,
        owner: ReferenceOwner,
        pending: &mut PendingPublication,
        input: &mut dyn Read,
        version: CheckpointFileVersion,
    ) -> Result<ChunkRoot, Error> {
        let mut root = ChunkRoot { count: 0, last: [0; 32] };
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
            let digest = self.publish_chunk(operation, owner, pending, &chunk[..count])?;
            let mut node = NODE_MAGIC.to_vec();
            node.extend_from_slice(&root.last);
            node.extend_from_slice(digest.as_bytes());
            root.last = *self.publish_chunk(operation, owner, pending, &node)?.as_bytes();
            root.count = root.count.checked_add(1).ok_or(ControlError::Capacity)?;
        }
        if version.bytes() != Some(total)
            || version.digest() != Some(peritus_types::Sha256Digest::new(hasher.finalize().into()))
        {
            return Err(Error::Corrupt("captured checkpoint body changed before publication"));
        }
        Ok(root)
    }

    pub(super) fn accept_restore_evidence(
        &mut self,
        operation: &ControlOperation,
        restore: peritus_product_runner::control::RestoreId,
        expected: [u8; 32],
        bytes: &[u8],
    ) -> Result<ControlReceipt, Error> {
        self.recover_snapshot_publications()?;
        let mut pending = PendingPublication::new(
            self.checkpoint_config.root(),
            RESTORE_EVIDENCE_NAMESPACE,
            restore.as_bytes(),
        )?;
        let version = CheckpointFileVersion::present(
            peritus_types::Sha256Digest::new(expected),
            bytes.len() as u64,
            peritus_product_runner::control::CheckpointFileMode::Regular,
        );
        let root = self.publish_stream(
            operation,
            reference_owner(RESTORE_EVIDENCE_NAMESPACE, restore.as_bytes()),
            &mut pending,
            &mut io::Cursor::new(bytes),
            version,
        )?;
        let record = EvidenceRoot { schema: 1, root, digest: expected, bytes: bytes.len() as u64 };
        let encoded = serde_json::to_vec(&record)
            .map_err(|_| Error::Corrupt("cannot encode restore evidence root"))?;
        #[cfg(test)]
        faults::check(SnapshotFaultPoint::BeforeRootPublication)?;
        let receipt = self.accept_installs(
            operation,
            vec![StateInstall::new(
                RESTORE_EVIDENCE_NAMESPACE,
                restore.as_bytes().to_vec(),
                None,
                1,
                encoded,
            )?],
        )?;
        #[cfg(test)]
        faults::check(SnapshotFaultPoint::AfterRootPublication)?;
        pending.finish()?;
        Ok(receipt)
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

    fn publish_chunk(
        &self,
        operation: &ControlOperation,
        owner: ReferenceOwner,
        pending: &mut PendingPublication,
        bytes: &[u8],
    ) -> Result<ArtifactDigest, Error> {
        let digest = ArtifactDigest::from_sha256(peritus_codec::sha256(bytes));
        let request = WriteRequest::new(
            digest,
            bytes.len() as u64,
            CHUNK_BYTES as u64,
            artifact(MediaType::new("application/octet-stream"))?,
            EncryptionMetadata::unencrypted(),
            super::super::event_id(operation)?,
        );
        let mut writer = artifact(self.checkpoint_artifacts.begin_write(request))?;
        artifact(writer.write_chunk(bytes))?;
        #[cfg(test)]
        faults::check(SnapshotFaultPoint::BeforeChunkFinalization)?;
        artifact(writer.finalize())?;
        pending.record(digest)?;
        artifact(self.checkpoint_artifacts.add_reference(owner, digest))?;
        Ok(digest)
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

fn artifact<T>(result: Result<T, peritus_artifact_store::ArtifactStoreError>) -> Result<T, Error> {
    result.map_err(|error| Error::Io(io::Error::other(error)))
}
fn mode(version: CheckpointFileVersion) -> Result<FileMode, Error> {
    match version.mode().ok_or(ControlError::InvalidInput)? {
        peritus_product_runner::control::CheckpointFileMode::Regular => Ok(FileMode::Regular),
        peritus_product_runner::control::CheckpointFileMode::Executable => Ok(FileMode::Executable),
    }
}
