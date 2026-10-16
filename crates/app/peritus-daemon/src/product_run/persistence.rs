//! Durable product-run snapshots and restart recovery.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

use peritus_app_protocol::{
    ProductProviderSelection, ProductRunPhase, ProductRunSnapshot, encode_workbench_result_value,
};
use peritus_product_runner::ProductRunResume;
use peritus_provider_core::CancellationToken;
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};

use super::progress::RunProgress;
use super::{
    PreviewAggregate, PreviewOperationRecord, ProductRunRequest, ProductRunServiceError, RunRecord,
};

use crate::DaemonError;

mod deliverable;
mod interaction;
mod preview;
mod progress;
use preview::restore_preview;
mod continuation;
mod settlement;
mod workbench;
pub(super) use workbench::load_workbench_records;

use settlement::{PersistedCheckpoint, restore_settlement};

mod types;
use types::{
    PersistedDeliverable, PersistedPreviewOperation, PersistedPreviewOutput, PersistedProgress,
    PersistedRecord,
};

pub(super) fn persist_record(
    directory: &Path,
    record: &RunRecord,
) -> Result<(), ProductRunServiceError> {
    let result = write_record(directory, record);
    if let Err(error) = &result {
        record.interaction.record_persistence_failure(error.describe());
        record.interaction.persistence_failed.store(true, std::sync::atomic::Ordering::Release);
        record.cancelled.store(true, std::sync::atomic::Ordering::Release);
        let _ = record.provider_cancellation.cancel();
    }
    result
}

pub(super) fn write_record(
    directory: &Path,
    record: &RunRecord,
) -> Result<(), ProductRunServiceError> {
    use std::io::Write as _;

    let workbench_directory = record_directory(directory)?;
    fs::create_dir_all(&workbench_directory).map_err(|error| {
        ProductRunServiceError::persistence("create the workbench run directory", error)
    })?;
    let directory = workbench_directory.as_path();
    let mut persisted = PersistedRecord::from_record(record)?;
    if let Some(bytes) = persisted.resume_state.take() {
        persisted.resume_artifact = Some(continuation::store(directory, &bytes)?);
    }
    let path = directory.join(format!("{}.json", persisted.run_id));
    let temporary = path.with_extension("json.new");
    let mut file = fs::File::create(&temporary).map_err(|error| {
        ProductRunServiceError::persistence("create the product-run temporary record", error)
    })?;

    {
        // The buffer bounds an I/O operation, not the lifetime of the retained run.
        let mut writer = std::io::BufWriter::new(&mut file);
        serde_json::to_writer(&mut writer, &persisted).map_err(|error| {
            ProductRunServiceError::persistence("serialize the product-run record", error)
        })?;
        writer.flush().map_err(|error| {
            ProductRunServiceError::persistence("write the product-run temporary record", error)
        })?;
    }

    file.sync_all().map_err(|error| {
        ProductRunServiceError::persistence("sync the product-run temporary record", error)
    })?;

    fs::rename(temporary, path).map_err(|error| {
        ProductRunServiceError::persistence("replace the durable product-run record", error)
    })?;

    #[cfg(unix)]
    {
        fs::File::open(directory).and_then(|file| file.sync_all()).map_err(|error| {
            ProductRunServiceError::persistence("sync the product-run directory", error)
        })?;
    }
    Ok(())
}

pub(super) fn record_directory(directory: &Path) -> Result<PathBuf, ProductRunServiceError> {
    if directory.file_name().is_some_and(|name| name == "product-runs") {
        return Ok(directory
            .parent()
            .ok_or_else(|| {
                ProductRunServiceError::internal(
                    "resolve the workbench run directory",
                    "the configured product-run directory has no parent",
                )
            })?
            .join("workbench-v1/runs"));
    }
    Ok(directory.to_path_buf())
}

pub(super) fn quarantine_record(path: &Path, reason: &str, source: Option<&dyn std::fmt::Display>) {
    if path.parent().and_then(Path::file_name).is_some_and(|name| name == ".quarantine") {
        return;
    }
    isolate_record(path, ".quarantine", reason, source);
}

pub(super) fn retire_record(path: &Path, reason: &str) {
    isolate_record(path, ".retired", reason, None);
}

fn isolate_record(
    path: &Path,
    directory_name: &str,
    reason: &str,
    source: Option<&dyn std::fmt::Display>,
) {
    let Some(parent) = path.parent() else {
        report_isolation(path, reason, source, None);
        return;
    };
    let isolation_directory = parent.join(directory_name);
    let result = fs::create_dir_all(&isolation_directory).and_then(|()| {
        let destination = available_isolation_path(&isolation_directory, path);
        fs::rename(path, destination)
    });
    report_isolation(path, reason, source, result.err().as_ref());
}

fn available_isolation_path(directory: &Path, source: &Path) -> PathBuf {
    let name = source.file_name().and_then(|value| value.to_str()).unwrap_or("run.json");
    let initial = directory.join(name);
    if !initial.exists() {
        return initial;
    }
    for suffix in 1_u32.. {
        let candidate = directory.join(format!("{name}.{suffix}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("u32 isolation suffixes are exhaustive")
}

fn report_isolation(
    path: &Path,
    reason: &str,
    source: Option<&dyn std::fmt::Display>,
    isolation_error: Option<&std::io::Error>,
) {
    let source = source.map_or(String::new(), |error| format!(": {error}"));
    let isolation = isolation_error
        .map_or(String::new(), |error| format!("; could not move the file aside: {error}"));
    crate::diagnostic::report(&format!(
        "peritusd: isolated unusable run projection {}: {reason}{source}{isolation}",
        path.display()
    ));
}

fn report_isolation_failure(path: &Path, operation: &str, error: &std::io::Error) {
    crate::diagnostic::report(&format!(
        "peritusd: skipped {} while attempting to {operation}: {error}",
        path.display()
    ));
}

fn record_path_matches(path: &Path, run_id: RunId) -> bool {
    path.file_name().and_then(|name| name.to_str())
        == Some(format!("{}.json", hex(run_id.as_bytes())).as_str())
}

mod record;
use record::hex;
