//! Exact immutable snapshot readers and retained-root integrity verification.

use super::{
    ArtifactDigest, BodyRoots, CHUNK_BYTES, ChunkRoot, ControlError, ControlStore, Error,
    NODE_MAGIC, SNAPSHOT_NAMESPACE, artifact, mode,
};
use peritus_artifact_store::{ArtifactStore, StoreConfig};
use peritus_patch::{SnapshotFile, SnapshotSource};
use peritus_product_runner::control::{CheckpointFileVersion, CheckpointId, UserCheckpoint};
use sha2::{Digest as _, Sha256};
use std::{
    fs::File,
    io::{self, Read, Write as _},
    sync::Arc,
};

impl ControlStore {
    pub(super) fn verify_stream(
        &self,
        root: ChunkRoot,
        version: CheckpointFileVersion,
    ) -> Result<(), Error> {
        let source = self.snapshot_source(root, version)?;
        let mut reader = source.open()?;
        let mut hasher = Sha256::new();
        let mut chunk = vec![0_u8; 64 * 1024];
        let mut total = 0_u64;
        loop {
            let count = reader.read(&mut chunk)?;
            if count == 0 {
                break;
            }
            total = total.checked_add(count as u64).ok_or(ControlError::Capacity)?;
            hasher.update(&chunk[..count]);
        }
        if version.bytes() != Some(total)
            || version.digest() != Some(peritus_types::Sha256Digest::new(hasher.finalize().into()))
        {
            return Err(Error::Corrupt("retained chunks differ from immutable manifest"));
        }
        Ok(())
    }

    pub(crate) fn checkpoint_snapshot(
        &self,
        checkpoint: CheckpointId,
        index: usize,
        version: CheckpointFileVersion,
    ) -> Result<Option<SnapshotFile>, Error> {
        let CheckpointFileVersion::Present { .. } = version else {
            return Ok(None);
        };
        let source: Arc<dyn SnapshotSource> = if let Some(record) =
            self.journal.state_record(SNAPSHOT_NAMESPACE, checkpoint.as_bytes())?
        {
            let roots = decode_roots(record.bytes())?;
            let root = roots
                .bodies
                .get(index)
                .copied()
                .flatten()
                .ok_or(Error::Corrupt("present checkpoint has no chunk root"))?;
            Arc::new(self.snapshot_source(root, version)?)
        } else {
            let bytes = self
                .checkpoint_body(checkpoint, index, version)?
                .ok_or(Error::Corrupt("legacy checkpoint body missing"))?;
            let mut file = tempfile::NamedTempFile::new()?;
            file.write_all(&bytes)?;
            Arc::new(LegacySource(file.into_temp_path()))
        };
        Ok(Some(SnapshotFile::from_source(
            source,
            version.digest().ok_or(ControlError::InvalidInput)?,
            version.bytes().ok_or(ControlError::InvalidInput)?,
            mode(version)?,
        )))
    }

    pub(in crate::product_control::storage::checkpoints) fn verify_snapshot_bodies(
        &self,
        checkpoint: &UserCheckpoint,
        position: u64,
    ) -> Result<bool, Error> {
        let Some(record) =
            self.journal.state_record(SNAPSHOT_NAMESPACE, checkpoint.id().as_bytes())?
        else {
            return Ok(false);
        };
        if record.revision() != 1 || record.producing_position() != position {
            return Err(Error::Corrupt("checkpoint chunk roots were not published atomically"));
        }
        let roots = decode_roots(record.bytes())?;
        if roots.bodies.len() != checkpoint.paths().len() {
            return Err(Error::Corrupt("checkpoint chunk root coverage differs"));
        }
        for (path, root) in checkpoint.paths().iter().zip(roots.bodies) {
            match (path.checkpoint(), root) {
                (
                    CheckpointFileVersion::Absent | CheckpointFileVersion::EmptyDirectory { .. },
                    None,
                ) => {}
                (version @ CheckpointFileVersion::Present { .. }, Some(root)) => {
                    self.verify_stream(root, version)?;
                }
                _ => return Err(Error::Corrupt("checkpoint chunk root presence differs")),
            }
        }
        Ok(true)
    }

    pub(super) fn snapshot_source(
        &self,
        root: ChunkRoot,
        version: CheckpointFileVersion,
    ) -> Result<ChunkSource, Error> {
        let size = version.bytes().ok_or(ControlError::InvalidInput)?;
        if root.count != size.div_ceil(CHUNK_BYTES as u64) {
            return Err(Error::Corrupt("checkpoint chunk count differs from size"));
        }
        let mut chunks = Vec::new();
        let mut visited = std::collections::BTreeSet::new();
        let mut next = root.last;
        for _ in 0..root.count {
            if !visited.insert(next) {
                return Err(Error::Corrupt("checkpoint chunk chain repeats a node"));
            }
            let node = artifact(self.checkpoint_artifacts.read(ArtifactDigest::new(next), 72))?;
            if node.len() != 72 || &node[..8] != NODE_MAGIC {
                return Err(Error::Corrupt("checkpoint chunk node invalid"));
            }
            next.copy_from_slice(&node[8..40]);
            let mut digest = [0_u8; 32];
            digest.copy_from_slice(&node[40..]);
            chunks.push(ArtifactDigest::new(digest));
        }
        if next != [0; 32] {
            return Err(Error::Corrupt("checkpoint chunk chain has excess nodes"));
        }
        chunks.reverse();
        Ok(ChunkSource { config: self.checkpoint_config.clone(), chunks })
    }
}

fn decode_roots(bytes: &[u8]) -> Result<BodyRoots, Error> {
    let roots: BodyRoots = serde_json::from_slice(bytes)
        .map_err(|_| Error::Corrupt("checkpoint chunk roots invalid"))?;
    if roots.schema != 1 {
        return Err(Error::Corrupt("unsupported checkpoint chunk root generation"));
    }
    Ok(roots)
}
#[derive(Debug)]
struct LegacySource(tempfile::TempPath);
impl SnapshotSource for LegacySource {
    fn open(&self) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(File::open(&self.0)?))
    }
}
#[derive(Debug)]
pub(super) struct ChunkSource {
    config: StoreConfig,
    chunks: Vec<ArtifactDigest>,
}
impl SnapshotSource for ChunkSource {
    fn open(&self) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(ChunkReader {
            config: self.config.clone(),
            chunks: self.chunks.clone().into_iter(),
            current: io::Cursor::new(Vec::new()),
        }))
    }
}
struct ChunkReader {
    config: StoreConfig,
    chunks: std::vec::IntoIter<ArtifactDigest>,
    current: io::Cursor<Vec<u8>>,
}
impl Read for ChunkReader {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            let count = self.current.read(output)?;
            if count != 0 {
                return Ok(count);
            }
            let Some(digest) = self.chunks.next() else {
                return Ok(0);
            };
            let bytes = ArtifactStore::read_existing(&self.config, digest, CHUNK_BYTES as u64)
                .map_err(io::Error::other)?;
            if bytes.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "empty checkpoint data chunk",
                ));
            }
            self.current = io::Cursor::new(bytes);
        }
    }
}
