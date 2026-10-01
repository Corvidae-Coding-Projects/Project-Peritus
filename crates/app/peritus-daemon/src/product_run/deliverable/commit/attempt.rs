//! Durable source identity for retrying a partially completed nested commit.

use super::{ProductDeliverable, ProductRunServiceError};
use crate::product_run::RunRecord;
use peritus_product_runner::ProductRunner;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

#[derive(Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Attempt {
    workspace: PathBuf,
    paths: [u8; 32],
    source: [u8; 32],
    patch: [u8; 32],
}

pub(in crate::product_run) fn matches(
    directory: &Path,
    record: &RunRecord,
    deliverable: &ProductDeliverable,
) -> Result<bool, ProductRunServiceError> {
    if deliverable.export_path().is_empty() {
        return Ok(false);
    }
    let path = path(directory, record);
    let file = match fs::File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(failure(error)),
    };
    let mut bytes = Vec::new();
    file.take(65537).read_to_end(&mut bytes).map_err(failure)?;
    if bytes.len() > 65536 {
        return Err(failure("commit recovery record exceeds its size limit"));
    }
    let expected: Attempt = serde_json::from_slice(&bytes).map_err(failure)?;
    Ok(expected == capture(deliverable)?)
}

pub(super) fn save(
    directory: &Path,
    record: &RunRecord,
    deliverable: &ProductDeliverable,
) -> Result<(), ProductRunServiceError> {
    let bytes = serde_json::to_vec(&capture(deliverable)?).map_err(failure)?;
    let path = path(directory, record);
    let temporary = path.with_extension("commit-attempt.new");
    let mut file = fs::File::create(&temporary).map_err(failure)?;
    file.write_all(&bytes).and_then(|()| file.sync_all()).map_err(failure)?;
    drop(file);
    fs::rename(&temporary, &path).map_err(failure)?;
    #[cfg(unix)]
    fs::File::open(directory).and_then(|file| file.sync_all()).map_err(failure)?;
    Ok(())
}

fn path(directory: &Path, record: &RunRecord) -> PathBuf {
    directory.join(format!("{}.commit-attempt", super::super::run_hex(record.request.run_id())))
}

fn capture(deliverable: &ProductDeliverable) -> Result<Attempt, ProductRunServiceError> {
    let workspace = PathBuf::from(deliverable.workspace_path());
    let paths =
        Sha256::digest(serde_json::to_vec(deliverable.changed_paths()).map_err(failure)?).into();
    let source = *ProductRunner::candidate_source_digest(&workspace).map_err(failure)?.as_bytes();
    let mut patch = fs::File::open(deliverable.export_path()).map_err(failure)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let count = patch.read(&mut buffer).map_err(failure)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(Attempt { workspace, paths, source, patch: hasher.finalize().into() })
}

fn failure(error: impl std::fmt::Display) -> ProductRunServiceError {
    ProductRunServiceError::internal("retain commit recovery", error.to_string())
}
