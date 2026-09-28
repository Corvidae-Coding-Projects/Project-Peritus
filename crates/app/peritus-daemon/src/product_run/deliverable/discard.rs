//! Durable acknowledgement of completed source restoration, independent of run projection saves.

use super::{ProductDeliverable, ProductRunServiceError};
use crate::product_run::RunRecord;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Completed {
    version: u8,
    binding: [u8; 32],
    status: String,
}

pub(super) fn save_completed(
    directory: &Path,
    record: &RunRecord,
    deliverable: &ProductDeliverable,
    status: &str,
) -> Result<(), ProductRunServiceError> {
    let completed =
        Completed { version: 1, binding: binding(record, deliverable)?, status: status.to_owned() };
    let bytes = serde_json::to_vec(&completed).map_err(failure)?;
    let path = path(directory, record);
    let temporary = path.with_extension("discard-result.new");
    let mut file = fs::File::create(&temporary).map_err(failure)?;
    file.write_all(&bytes).and_then(|()| file.sync_all()).map_err(failure)?;
    drop(file);
    fs::rename(&temporary, &path).map_err(failure)?;
    #[cfg(unix)]
    fs::File::open(directory).and_then(|file| file.sync_all()).map_err(failure)?;
    Ok(())
}

pub(in crate::product_run) fn recover_completed(
    directory: &Path,
    record: &mut RunRecord,
) -> Result<bool, ProductRunServiceError> {
    let Some(deliverable) = record.snapshot.deliverable() else { return Ok(false) };
    if deliverable.discarded() || !deliverable.commit_revision().is_empty() {
        return Ok(false);
    }
    let file = match fs::File::open(path(directory, record)) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(failure(error)),
    };
    let maximum = peritus_app_protocol::MAX_PRODUCT_DETAIL_BYTES.saturating_mul(6) + 1024;
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1).read_to_end(&mut bytes).map_err(failure)?;
    if bytes.len() > maximum {
        return Err(failure("discard completion record exceeds its size limit"));
    }
    let completed: Completed = serde_json::from_slice(&bytes).map_err(failure)?;
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
    Ok(true)
}

fn binding(
    record: &RunRecord,
    deliverable: &ProductDeliverable,
) -> Result<[u8; 32], ProductRunServiceError> {
    let checkpoint = record.checkpoint.map(|checkpoint| {
        let identity = checkpoint.identity();
        (
            identity.candidate_digest().into_bytes(),
            identity.conversation_revision(),
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
