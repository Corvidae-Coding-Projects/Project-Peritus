//! Bounded filesystem helpers for retained release evidence.

use std::{
    fs::{self, File},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

use peritus_release_artifacts::Sha256Digest;
use sha2::{Digest as _, Sha256};

use crate::error::OperatorError;

const MAX_METADATA_BYTES: u64 = 32 * 1024 * 1024;

pub fn read_metadata(path: &Path) -> Result<Vec<u8>, OperatorError> {
    let metadata = fs::metadata(path)
        .map_err(|error| OperatorError::io("inspect release metadata", path, error))?;
    if !metadata.is_file() || metadata.len() > MAX_METADATA_BYTES {
        return Err(OperatorError::metadata(format!(
            "release metadata {} is not a regular file at most 32 MiB",
            path.display()
        )));
    }
    fs::read(path).map_err(|error| OperatorError::io("read release metadata", path, error))
}

pub fn digest_file(path: &Path) -> Result<(u64, Sha256Digest), OperatorError> {
    let mut file = File::open(path)
        .map_err(|error| OperatorError::io("open release artifact", path, error))?;
    let metadata = file
        .metadata()
        .map_err(|error| OperatorError::io("inspect release artifact", path, error))?;
    if !metadata.is_file() {
        return Err(OperatorError::metadata(format!(
            "release artifact {} is not a regular file",
            path.display()
        )));
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| OperatorError::io("read release artifact", path, error))?;
        if count == 0 {
            return Ok((metadata.len(), Sha256Digest::from_bytes(hasher.finalize().into())));
        }
        hasher.update(&buffer[..count]);
    }
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), OperatorError> {
    let parent =
        path.parent().ok_or_else(|| OperatorError::metadata("evidence output has no parent"))?;
    fs::create_dir_all(parent)
        .map_err(|error| OperatorError::io("create evidence directory", parent, error))?;
    let temporary = path.with_extension(format!(
        "{}.tmp-{}",
        path.extension().and_then(|value| value.to_str()).unwrap_or("evidence"),
        std::process::id()
    ));
    let mut file = File::create(&temporary)
        .map_err(|error| OperatorError::io("create temporary evidence", &temporary, error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| OperatorError::io("write temporary evidence", &temporary, error))?;
    fs::rename(&temporary, path)
        .map_err(|error| OperatorError::io("publish retained evidence", path, error))
}

pub fn sibling(path: &Path, suffix: &str) -> Result<PathBuf, OperatorError> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| OperatorError::metadata("release artifact name is not UTF-8"))?;
    Ok(path.with_file_name(format!("{name}{suffix}")))
}
