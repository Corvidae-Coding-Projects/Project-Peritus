//! Durable acknowledgement of completed source restoration, independent of run projection saves.

use super::{ProductDeliverable, ProductRunServiceError};
use crate::product_run::RunRecord;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

mod reservation;
mod transaction;
pub(super) use reservation::Reservation;
pub(in crate::product_run) use transaction::{Pending, workspace_available};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Completed {
    version: u8,
    binding: [u8; 32],
    status: String,
}

#[cfg(test)]
pub(super) fn save_completed(
    directory: &Path,
    record: &RunRecord,
    deliverable: &ProductDeliverable,
    status: &str,
) -> Result<(), ProductRunServiceError> {
    Reservation::prepare(directory, record, deliverable)?.complete(status)
}

pub(in crate::product_run) fn recover_completed(
    directory: &Path,
    record: &mut RunRecord,
) -> Result<bool, ProductRunServiceError> {
    let Some(deliverable) = record.snapshot.deliverable() else { return Ok(false) };
    if deliverable.discarded() || !deliverable.commit_revision().is_empty() {
        return Ok(false);
    }
    let completed = if let Some(completed) = read_completed(directory, record)? {
        completed
    } else if let Some(pending) = Pending::read(directory, record)? {
        let peritus_product_runner::DiscardTransactionState::Completed(paths) =
            pending.inspect(directory, record)?
        else {
            return Ok(false);
        };
        Completed {
            version: 1,
            binding: binding(record, deliverable)?,
            status: super::discard_status(&paths),
        }
    } else {
        return Ok(false);
    };
    if completed.version != 1 {
        return Err(failure("unsupported discard completion record version"));
    }
    if completed.binding != binding(record, deliverable)? {
        return Ok(false);
    }
    let deliverable = deliverable.clone().mark_discarded();
    record.snapshot = super::replace_snapshot(
        &record.snapshot,
        record.snapshot.phase(),
        &completed.status,
        record.snapshot.summary(),
    )?
    .with_deliverable(deliverable);
    Pending::clear_interruption(record);
    Ok(true)
}

fn read_completed(
    directory: &Path,
    record: &RunRecord,
) -> Result<Option<Completed>, ProductRunServiceError> {
    let final_path = path(directory, record);
    if let Some(bytes) = read_record(&final_path)? {
        return serde_json::from_slice(&bytes).map(Some).map_err(failure);
    }
    // A fully written completion can survive a failed rename. A reservation alone
    // never proves that restoration happened and cannot acknowledge a discard.
    let Some(bytes) = read_record(&final_path.with_extension("discard-result.new"))? else {
        return Ok(None);
    };
    if serde_json::from_slice::<reservation::Prepared>(&bytes).is_ok() {
        return Ok(None);
    }
    serde_json::from_slice(&bytes).map(Some).map_err(failure)
}

fn read_record(path: &Path) -> Result<Option<Vec<u8>>, ProductRunServiceError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(failure(error)),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(failure("discard record is not an ordinary file"));
    }
    fs::read(path).map(Some).map_err(failure)
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> Result<(), ProductRunServiceError> {
    fs::File::open(directory).and_then(|file| file.sync_all()).map_err(failure)?;
    Ok(())
}

#[cfg(not(unix))]
const fn sync_directory(_directory: &Path) -> Result<(), ProductRunServiceError> {
    Ok(())
}

fn binding(
    record: &RunRecord,
    deliverable: &ProductDeliverable,
) -> Result<[u8; 32], ProductRunServiceError> {
    let checkpoint = record.checkpoint.map(|checkpoint| {
        let identity = checkpoint.identity();
        (
            identity.repository_digest().into_bytes(),
            identity.requirements_revision(),
            identity.checkpoint_sequence(),
        )
    });
    let encoded = serde_json::to_vec(&(
        record.request.run_id().into_bytes(),
        record.request.workspace_id().into_bytes(),
        deliverable.workspace_path(),
        deliverable.changed_paths(),
        record.task_baseline_required,
        &record.task_baseline,
        checkpoint,
    ))
    .map_err(failure)?;
    Ok(Sha256::digest(encoded).into())
}

fn path(directory: &Path, record: &RunRecord) -> PathBuf {
    directory.join(format!("{}.discard-result", super::run_hex(record.request.run_id())))
}

fn failure(error: impl std::fmt::Display) -> ProductRunServiceError {
    ProductRunServiceError::internal("retain completed discard result", error.to_string())
}
