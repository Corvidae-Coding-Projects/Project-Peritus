//! Bounded artifact hashing tied to the exact open file and durable metadata.

use crate::{EvidenceError, EvidenceErrorKind, RecoveryAction};
use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, Read},
};

pub struct ArtifactInput {
    file: File,
    digest: ArtifactDigest,
    size: u64,
    offset: u64,
    hash: Sha256,
}

impl ArtifactInput {
    pub(crate) fn open(
        store: &ArtifactStore,
        digest: ArtifactDigest,
    ) -> Result<Self, EvidenceError> {
        let metadata = store
            .metadata(digest)
            .map_err(|e| EvidenceError::artifact("read artifact metadata", e))?
            .filter(peritus_artifact_store::ArtifactMetadata::is_referenceable)
            .ok_or_else(|| {
                EvidenceError::new(
                    EvidenceErrorKind::MissingArtifact,
                    RecoveryAction::RepairDependency,
                    "open evidence artifact",
                    "artifact is not finalized and active",
                )
            })?;
        let hex = digest.to_hex();
        let path = store.root().join("objects").join("sha256").join(&hex[..2]).join(hex);
        let stat = fs::symlink_metadata(&path).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                EvidenceError::new(
                    EvidenceErrorKind::MissingArtifact,
                    RecoveryAction::RepairDependency,
                    "inspect evidence artifact",
                    "artifact object is missing",
                )
            } else {
                EvidenceError::io("inspect evidence artifact", e)
            }
        })?;
        if !stat.file_type().is_file() {
            return Err(corrupt("artifact is not a regular file"));
        }
        let file = File::open(path).map_err(|e| EvidenceError::io("open evidence artifact", e))?;
        if file
            .metadata()
            .map_err(|e| EvidenceError::io("inspect open evidence artifact", e))?
            .len()
            != metadata.size()
        {
            return Err(corrupt("artifact size differs from durable metadata"));
        }
        Ok(Self { file, digest, size: metadata.size(), offset: 0, hash: Sha256::new() })
    }
    pub(crate) const fn size(&self) -> u64 {
        self.size
    }
    pub(crate) const fn offset(&self) -> u64 {
        self.offset
    }
    pub(crate) fn read(&mut self, buffer: &mut [u8]) -> Result<usize, EvidenceError> {
        let wanted = usize::try_from((self.size - self.offset).min(buffer.len() as u64))
            .map_err(|_| corrupt("artifact read size overflows"))?;
        if wanted == 0 {
            if self
                .file
                .metadata()
                .map_err(|e| EvidenceError::io("finish evidence artifact", e))?
                .len()
                != self.size
                || self.digest.as_bytes() != self.hash.clone().finalize().as_slice()
            {
                return Err(corrupt("artifact size or content digest mismatch"));
            }
            return Ok(0);
        }
        let read = loop {
            match self.file.read(&mut buffer[..wanted]) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Err(EvidenceError::io("read evidence artifact", error)),
                Ok(0) => return Err(corrupt("artifact ended before its durable size")),
                Ok(read) => break read,
            }
        };
        self.hash.update(&buffer[..read]);
        self.offset += read as u64;
        Ok(read)
    }
}
fn corrupt(detail: &'static str) -> EvidenceError {
    EvidenceError::new(
        EvidenceErrorKind::CorruptArtifact,
        RecoveryAction::RepairDependency,
        "verify evidence artifact",
        detail,
    )
}
