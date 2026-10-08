//! Digest-bound checkpoint manifest pages and stable path-to-body associations.

use super::{
    BodyRoots, ChunkRoot, ControlError, ControlOperation, ControlStore, Error, EvidenceRoot,
    PendingPublication, ReferenceOwner,
};
use peritus_patch::SnapshotSource as _;
use peritus_product_runner::control::{
    CheckpointFileMode, CheckpointFileVersion, CheckpointId, UserCheckpoint,
};
use peritus_types::Sha256Digest;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::io::{self, Read as _};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PagedBodyRoots {
    schema: u16,
    entries: u64,
    manifest: EvidenceRoot,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotManifest {
    schema: u16,
    checkpoint: UserCheckpoint,
    bodies: Vec<BodyEntry>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BodyEntry {
    path_id: [u8; 32],
    root: Option<ChunkRoot>,
}

enum StoredBodyRoots {
    Legacy(BodyRoots),
    Paged(PagedBodyRoots),
}

pub(super) struct DecodedBodyRoots {
    pub(super) captured: Option<UserCheckpoint>,
    pub(super) bodies: Vec<Option<ChunkRoot>>,
}

impl ControlStore {
    pub(super) fn publish_body_manifest(
        &self,
        operation: &ControlOperation,
        owner: ReferenceOwner,
        pending: &mut PendingPublication,
        checkpoint: &UserCheckpoint,
        roots: Vec<Option<ChunkRoot>>,
    ) -> Result<Vec<u8>, Error> {
        if roots.len() != checkpoint.paths().len() {
            return Err(ControlError::InvalidInput.into());
        }
        let bodies = checkpoint
            .paths()
            .iter()
            .zip(roots)
            .map(|(path, root)| BodyEntry { path_id: path.path_id().into_bytes(), root })
            .collect();
        let manifest = SnapshotManifest { schema: 1, checkpoint: checkpoint.clone(), bodies };
        let bytes = serde_json::to_vec(&manifest)
            .map_err(|_| Error::Corrupt("cannot encode checkpoint manifest pages"))?;
        let digest = peritus_codec::sha256(&bytes);
        let size = u64::try_from(bytes.len()).map_err(|_| ControlError::Capacity)?;
        let version = CheckpointFileVersion::present(digest, size, CheckpointFileMode::Regular);
        let root =
            self.publish_stream(operation, owner, pending, &mut io::Cursor::new(&bytes), version)?;
        serde_json::to_vec(&PagedBodyRoots {
            schema: 2,
            entries: u64::try_from(checkpoint.paths().len()).map_err(|_| ControlError::Capacity)?,
            manifest: EvidenceRoot { schema: 1, root, digest: digest.into_bytes(), bytes: size },
        })
        .map_err(|_| Error::Corrupt("cannot encode checkpoint manifest root"))
    }

    pub(super) fn decode_body_roots(
        &self,
        checkpoint: CheckpointId,
        bytes: &[u8],
    ) -> Result<DecodedBodyRoots, Error> {
        let shape: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|_| Error::Corrupt("checkpoint body root invalid"))?;
        // Select the generation without retrying incompatible schemas, then decode the original
        // bytes so strict structs still reject duplicate and unknown fields.
        let stored = match shape.get("schema").and_then(serde_json::Value::as_u64) {
            Some(1) => StoredBodyRoots::Legacy(
                serde_json::from_slice(bytes)
                    .map_err(|_| Error::Corrupt("checkpoint body root invalid"))?,
            ),
            Some(2) => StoredBodyRoots::Paged(
                serde_json::from_slice(bytes)
                    .map_err(|_| Error::Corrupt("checkpoint body root invalid"))?,
            ),
            _ => return Err(Error::Corrupt("unsupported checkpoint body root generation")),
        };
        match stored {
            StoredBodyRoots::Legacy(roots) if roots.schema == 1 => {
                Ok(DecodedBodyRoots { captured: None, bodies: roots.bodies })
            }
            StoredBodyRoots::Paged(root) if root.schema == 2 && root.manifest.schema == 1 => {
                let bytes = self.manifest_bytes(&root.manifest)?;
                let manifest: SnapshotManifest = serde_json::from_slice(&bytes)
                    .map_err(|_| Error::Corrupt("checkpoint manifest pages invalid"))?;
                if manifest.schema != 1
                    || manifest.checkpoint.id() != checkpoint
                    || manifest.bodies.len() != manifest.checkpoint.paths().len()
                    || u64::try_from(manifest.bodies.len()).ok() != Some(root.entries)
                {
                    return Err(Error::Corrupt("checkpoint manifest identity or coverage differs"));
                }
                let mut bodies = Vec::new();
                bodies.try_reserve(manifest.bodies.len()).map_err(capacity)?;
                for (path, entry) in manifest.checkpoint.paths().iter().zip(manifest.bodies) {
                    if path.path_id().as_bytes() != &entry.path_id
                        || path.checkpoint().bytes().is_some() != entry.root.is_some()
                    {
                        return Err(Error::Corrupt(
                            "checkpoint manifest body path identity differs",
                        ));
                    }
                    bodies.push(entry.root);
                }
                Ok(DecodedBodyRoots { captured: Some(manifest.checkpoint), bodies })
            }
            _ => Err(Error::Corrupt("unsupported checkpoint body root generation")),
        }
    }

    pub(super) fn manifest_bytes(&self, manifest: &EvidenceRoot) -> Result<Vec<u8>, Error> {
        let version = CheckpointFileVersion::present(
            Sha256Digest::new(manifest.digest),
            manifest.bytes,
            CheckpointFileMode::Regular,
        );
        let mut input = self.snapshot_source(manifest.root, version)?.open()?;
        let mut output = Vec::new();
        let mut buffer = vec![0_u8; 64 * 1024];
        let mut hasher = Sha256::new();
        let mut total = 0_u64;
        loop {
            let count = input.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            total = total.checked_add(count as u64).ok_or(ControlError::Capacity)?;
            if total > manifest.bytes {
                return Err(Error::Corrupt("checkpoint manifest exceeds its exact byte identity"));
            }
            output.try_reserve(count).map_err(capacity)?;
            output.extend_from_slice(&buffer[..count]);
            hasher.update(&buffer[..count]);
        }
        let digest: [u8; 32] = hasher.finalize().into();
        if total != manifest.bytes || digest != manifest.digest {
            return Err(Error::Corrupt("checkpoint manifest digest or length differs"));
        }
        Ok(output)
    }
}

fn capacity(error: std::collections::TryReserveError) -> Error {
    Error::Io(io::Error::other(error))
}
