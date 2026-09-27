//! Durable product-run snapshots and restart recovery.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

use peritus_app_protocol::{
    ProductConversationMessage, ProductConversationRole, ProductProviderSelection, ProductRunPhase,
    ProductRunRequest, ProductRunSnapshot, encode_workbench_result_value,
};
use peritus_product_runner::{ConversationView, ProductRunResume};
use peritus_provider_core::CancellationToken;
use peritus_types::{ProviderProfileId, RunId, WorkspaceId};

use super::progress::RunProgress;
use super::{
    PreviewAggregate, PreviewOperationRecord, ProductRunServiceError, RunRecord,
    SharedConversation, filesystem,
};
use crate::DaemonError;

mod deliverable;
mod interaction;
mod preview;
mod progress;
use preview::restore_preview;
mod settlement;
mod workbench;
pub(super) use workbench::load_workbench_records;

use settlement::{PersistedCheckpoint, restore_settlement};

mod types;
use types::{
    PersistedDeliverable, PersistedMessage, PersistedPreviewOperation, PersistedPreviewOutput,
    PersistedProgress, PersistedRecord,
};

const MAX_PREVIEW_OPERATIONS: usize = 16_384;
const MAX_PREVIEW_OUTPUT_BYTES: usize = 4 * 1_024 * 1_024;

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistenceFaultPoint {
    BeforeWrite,
    BeforeFileSync,
    BeforeRename,
    AfterRename,
    BeforeDirectorySync,
}

#[cfg(test)]
static PERSISTENCE_FAULTS: std::sync::Mutex<Vec<([u8; 16], PersistenceFaultPoint)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
static PERSISTENT_PERSISTENCE_FAULTS: std::sync::Mutex<Vec<([u8; 16], PersistenceFaultPoint)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub fn inject_persistence_fault(run_id: RunId, point: PersistenceFaultPoint) {
    PERSISTENCE_FAULTS.lock().expect("persistence fault lock").push((run_id.into_bytes(), point));
}

#[cfg(test)]
pub fn inject_persistent_persistence_fault(run_id: RunId, point: PersistenceFaultPoint) {
    PERSISTENT_PERSISTENCE_FAULTS
        .lock()
        .expect("persistent persistence fault lock")
        .push((run_id.into_bytes(), point));
}

#[cfg(test)]
pub fn clear_persistent_persistence_fault(run_id: RunId, point: PersistenceFaultPoint) {
    PERSISTENT_PERSISTENCE_FAULTS
        .lock()
        .expect("persistent persistence fault lock")
        .retain(|candidate| candidate != &(run_id.into_bytes(), point));
}

#[cfg(test)]
fn check_persistence_fault(
    run_id: RunId,
    point: PersistenceFaultPoint,
) -> Result<(), ProductRunServiceError> {
    if PERSISTENT_PERSISTENCE_FAULTS
        .lock()
        .map_err(|_| {
            ProductRunServiceError::internal(
                "read the persistent persistence fault schedule",
                "the test fault lock was poisoned",
            )
        })?
        .contains(&(run_id.into_bytes(), point))
    {
        return Err(ProductRunServiceError::persistence(
            persistence_fault_operation(point),
            "injected persistent persistence failure",
        ));
    }
    let mut faults = PERSISTENCE_FAULTS.lock().map_err(|_| {
        ProductRunServiceError::internal(
            "read the persistence fault schedule",
            "the test fault lock was poisoned",
        )
    })?;
    if let Some(index) =
        faults.iter().position(|candidate| candidate == &(run_id.into_bytes(), point))
    {
        faults.remove(index);
        return Err(ProductRunServiceError::persistence(
            persistence_fault_operation(point),
            "injected persistence failure",
        ));
    }
    Ok(())
}

pub(super) fn persist_record(
    directory: &Path,
    record: &RunRecord,
) -> Result<(), ProductRunServiceError> {
    let result = write_record(directory, record);
    if let Err(error) = &result
        && let Some(options) = &record.interaction
    {
        options.record_persistence_failure(error.describe());
        options.persistence_failed.store(true, std::sync::atomic::Ordering::Release);
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
    let workbench_directory;
    let workbench = record.interaction.as_ref().is_some_and(|options| options.workbench.is_some());
    let directory = if workbench {
        workbench_directory = directory
            .parent()
            .ok_or_else(|| {
                ProductRunServiceError::internal(
                    "resolve the workbench run directory",
                    "the configured product-run directory has no parent",
                )
            })?
            .join("workbench-v1")
            .join("runs");
        fs::create_dir_all(&workbench_directory).map_err(|error| {
            ProductRunServiceError::persistence("create the workbench run directory", error)
        })?;
        workbench_directory.as_path()
    } else {
        directory
    };
    let persisted = PersistedRecord::from_record(record)?;
    let bytes = serde_json::to_vec_pretty(&persisted).map_err(|error| {
        ProductRunServiceError::persistence("serialize the product-run record", error)
    })?;
    if workbench && bytes.len() as u64 > workbench::MAX_RUN_RECORD_BYTES {
        return Err(ProductRunServiceError::persistence(
            "serialize the workbench run record",
            format!(
                "record is {} bytes; the durable limit is {} bytes",
                bytes.len(),
                workbench::MAX_RUN_RECORD_BYTES
            ),
        ));
    }
    let path = directory.join(format!("{}.json", persisted.run_id));
    if workbench {
        workbench::make_room_for_record(directory, &path)?;
    }
    let temporary = path.with_extension("json.new");
    let mut file = fs::File::create(&temporary).map_err(|error| {
        ProductRunServiceError::persistence("create the product-run temporary record", error)
    })?;
    #[cfg(test)]
    check_persistence_fault(record.request.run_id(), PersistenceFaultPoint::BeforeWrite)?;
    file.write_all(&bytes).map_err(|error| {
        ProductRunServiceError::persistence("write the product-run temporary record", error)
    })?;
    #[cfg(test)]
    check_persistence_fault(record.request.run_id(), PersistenceFaultPoint::BeforeFileSync)?;
    file.sync_all().map_err(|error| {
        ProductRunServiceError::persistence("sync the product-run temporary record", error)
    })?;
    #[cfg(test)]
    check_persistence_fault(record.request.run_id(), PersistenceFaultPoint::BeforeRename)?;
    fs::rename(temporary, path).map_err(|error| {
        ProductRunServiceError::persistence("replace the durable product-run record", error)
    })?;
    #[cfg(test)]
    check_persistence_fault(record.request.run_id(), PersistenceFaultPoint::AfterRename)?;
    #[cfg(unix)]
    {
        #[cfg(test)]
        check_persistence_fault(
            record.request.run_id(),
            PersistenceFaultPoint::BeforeDirectorySync,
        )?;
        fs::File::open(directory).and_then(|file| file.sync_all()).map_err(|error| {
            ProductRunServiceError::persistence("sync the product-run directory", error)
        })?;
    }
    Ok(())
}

#[cfg(test)]
const fn persistence_fault_operation(point: PersistenceFaultPoint) -> &'static str {
    match point {
        PersistenceFaultPoint::BeforeWrite => "write the product-run temporary record",
        PersistenceFaultPoint::BeforeFileSync => "sync the product-run temporary record",
        PersistenceFaultPoint::BeforeRename => "replace the durable product-run record",
        PersistenceFaultPoint::AfterRename => "complete durable product-run replacement",
        PersistenceFaultPoint::BeforeDirectorySync => "sync the product-run directory",
    }
}

pub(super) fn load_records(directory: &Path) -> Result<BTreeMap<RunId, RunRecord>, DaemonError> {
    let mut records = BTreeMap::new();
    for entry in fs::read_dir(directory).map_err(filesystem)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                report_isolation_failure(directory, "enumerate a product-run record", &error);
                continue;
            }
        };
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                quarantine_record(&path, "product-run record cannot be read", Some(&error));
                continue;
            }
        };
        let persisted: PersistedRecord = match serde_json::from_slice(&bytes) {
            Ok(persisted) => persisted,
            Err(error) => {
                quarantine_record(&path, "product-run state is malformed", Some(&error));
                continue;
            }
        };
        let record = match persisted.into_record() {
            Ok(record) => record,
            Err(error) => {
                quarantine_record(&path, "product-run state contains invalid values", Some(&error));
                continue;
            }
        };
        if !record_path_matches(&path, record.request.run_id()) {
            quarantine_record(
                &path,
                "product-run filename does not match its embedded identity",
                None,
            );
            continue;
        }
        if record.interaction.as_ref().is_some_and(|options| options.workbench.is_some()) {
            quarantine_record(
                &path,
                "workbench execution state appeared in the legacy generation",
                None,
            );
            continue;
        }
        if records.contains_key(&record.request.run_id()) {
            quarantine_record(&path, "duplicate product-run identity", None);
            continue;
        }
        records.insert(record.request.run_id(), record);
    }
    Ok(records)
}

pub(super) fn quarantine_record(path: &Path, reason: &str, source: Option<&dyn std::fmt::Display>) {
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
    eprintln!(
        "peritusd: isolated unusable run projection {}: {reason}{source}{isolation}",
        path.display()
    );
}

fn report_isolation_failure(path: &Path, operation: &str, error: &std::io::Error) {
    eprintln!("peritusd: skipped {} while attempting to {operation}: {error}", path.display());
}

fn record_path_matches(path: &Path, run_id: RunId) -> bool {
    path.file_name().and_then(|name| name.to_str())
        == Some(format!("{}.json", hex(run_id.as_bytes())).as_str())
}

mod record;
use record::hex;

#[cfg(test)]
#[path = "persistence/tests.rs"]
mod tests;
