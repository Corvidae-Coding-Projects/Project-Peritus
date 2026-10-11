//! Digest-addressed continuation bytes published before their small atomic run-record pointer.

use super::super::ProductRunServiceError;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ContinuationArtifact {
    digest: [u8; 32],
    bytes: u64,
}

fn artifact_path(runs: &Path, digest: &[u8; 32]) -> Result<PathBuf, ProductRunServiceError> {
    use std::fmt::Write as _;
    let parent = runs.parent().ok_or_else(|| {
        ProductRunServiceError::internal(
            "resolve continuation storage",
            "run directory has no parent",
        )
    })?;
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(parent.join("continuations").join(format!("{hex}.bin")))
}

pub(super) fn store(
    runs: &Path,
    bytes: &[u8],
) -> Result<ContinuationArtifact, ProductRunServiceError> {
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    let artifact = ContinuationArtifact { digest, bytes: bytes.len() as u64 };
    let path = artifact_path(runs, &digest)?;
    let directory = path.parent().ok_or(ProductRunServiceError::Unavailable)?;
    fs::create_dir_all(directory)
        .map_err(|e| ProductRunServiceError::persistence("create continuation directory", e))?;
    // Atomically replace an existing object too: an interrupted or damaged object cannot
    // make a fresh correct save point at corrupt bytes just because its name exists.
    let mut temporary = tempfile::NamedTempFile::new_in(directory)
        .map_err(|e| ProductRunServiceError::persistence("create continuation artifact", e))?;
    temporary
        .write_all(bytes)
        .and_then(|()| temporary.as_file().sync_all())
        .map_err(|e| ProductRunServiceError::persistence("sync continuation artifact", e))?;
    temporary
        .persist(&path)
        .map_err(|e| ProductRunServiceError::persistence("publish continuation artifact", e))?;
    #[cfg(unix)]
    fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|e| ProductRunServiceError::persistence("sync continuation directory", e))?;
    Ok(artifact)
}

pub(super) fn load(
    runs: &Path,
    artifact: &ContinuationArtifact,
) -> Result<Vec<u8>, ProductRunServiceError> {
    let path = artifact_path(runs, &artifact.digest)?;
    let metadata = fs::symlink_metadata(&path)
        .map_err(|e| ProductRunServiceError::persistence("inspect continuation artifact", e))?;
    if !metadata.is_file() || metadata.len() != artifact.bytes {
        return Err(ProductRunServiceError::internal(
            "validate continuation artifact",
            "continuation type or exact length does not match the saved pointer",
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(artifact.bytes.saturating_add(1)).read_to_end(&mut bytes))
        .map_err(|e| ProductRunServiceError::persistence("read continuation artifact", e))?;
    if bytes.len() as u64 != artifact.bytes
        || <[u8; 32]>::from(Sha256::digest(&bytes)) != artifact.digest
    {
        return Err(ProductRunServiceError::internal(
            "validate continuation artifact",
            "continuation digest or exact length does not match the saved pointer",
        ));
    }
    Ok(bytes)
}
