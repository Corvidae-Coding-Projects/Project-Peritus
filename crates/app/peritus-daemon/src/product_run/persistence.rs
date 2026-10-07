//! Durable product-run snapshots and restart recovery.

#[cfg(test)]
use std::collections::BTreeMap;
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
#[cfg(test)]
use super::{filesystem, invalid};
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
    PersistedDeliverable, PersistedPreviewOperation, PersistedPreviewOutput, PersistedProgress,
    PersistedRecord,
};

#[cfg(test)]
mod fault;
#[cfg(test)]
use fault::check_persistence_fault;
#[cfg(test)]
pub(super) use fault::{
    PersistenceFaultPoint, clear_persistent_persistence_fault, inject_persistence_fault,
    inject_persistent_persistence_fault,
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
    #[cfg(test)]
    let fault_directory = directory;
    let workbench_directory = record_directory(directory)?;
    fs::create_dir_all(&workbench_directory).map_err(|error| {
        ProductRunServiceError::persistence("create the workbench run directory", error)
    })?;
    let directory = workbench_directory.as_path();
    let persisted = PersistedRecord::from_record(record)?;
    let path = directory.join(format!("{}.json", persisted.run_id));
    let temporary = path.with_extension("json.new");
    let mut file = fs::File::create(&temporary).map_err(|error| {
        ProductRunServiceError::persistence("create the product-run temporary record", error)
    })?;
    #[cfg(test)]
    check_persistence_fault(
        fault_directory,
        record.request.run_id(),
        PersistenceFaultPoint::BeforeWrite,
    )?;
    // One physical I/O buffer never limits the logical retained record. Stream the existing
    // JSON layout instead of allocating a second, arbitrarily capped complete encoding.
    {
        let mut writer = std::io::BufWriter::with_capacity(32 * 1024, &mut file);
        serde_json::to_writer_pretty(&mut writer, &persisted).map_err(|error| {
            ProductRunServiceError::persistence("write the product-run temporary record", error)
        })?;
        writer.flush().map_err(|error| {
            ProductRunServiceError::persistence("flush the product-run temporary record", error)
        })?;
    }
    #[cfg(test)]
    check_persistence_fault(
        fault_directory,
        record.request.run_id(),
        PersistenceFaultPoint::BeforeFileSync,
    )?;
    file.sync_all().map_err(|error| {
        ProductRunServiceError::persistence("sync the product-run temporary record", error)
    })?;
    #[cfg(test)]
    check_persistence_fault(
        fault_directory,
        record.request.run_id(),
        PersistenceFaultPoint::BeforeRename,
    )?;
    fs::rename(temporary, path).map_err(|error| {
        ProductRunServiceError::persistence("replace the durable product-run record", error)
    })?;
    #[cfg(test)]
    check_persistence_fault(
        fault_directory,
        record.request.run_id(),
        PersistenceFaultPoint::AfterRename,
    )?;
    #[cfg(unix)]
    {
        #[cfg(test)]
        check_persistence_fault(
            fault_directory,
            record.request.run_id(),
            PersistenceFaultPoint::BeforeDirectorySync,
        )?;
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

#[cfg(test)]
pub(super) fn load_records(directory: &Path) -> Result<BTreeMap<RunId, RunRecord>, DaemonError> {
    let root = directory
        .parent()
        .filter(|_| directory.file_name().is_some_and(|name| name == "product-runs"))
        .map_or_else(
            || directory.parent().unwrap_or(directory).to_path_buf(),
            |parent| parent.join("workbench-v1"),
        );
    let controls = crate::product_control::ControlStore::open(
        &root,
        peritus_journal::StoreId::new([0x7f; 16]).map_err(|_| invalid("invalid test store"))?,
    )
    .map_err(|error| {
        DaemonError::with_source(
            crate::DaemonErrorCode::Storage,
            crate::DaemonRecovery::Reconcile,
            "open governed test state",
            "the durable workbench control store could not be reopened",
            error,
        )
    })?;
    load_workbench_records(&root, Some(&controls))
}

#[cfg(test)]
pub(super) fn load_unchecked_records(
    directory: &Path,
) -> Result<BTreeMap<RunId, RunRecord>, DaemonError> {
    let workbench_directory = record_directory(directory).map_err(|error| {
        DaemonError::new(
            crate::DaemonErrorCode::Storage,
            crate::DaemonRecovery::Reconcile,
            "resolve product-run state",
            error.describe(),
        )
    })?;
    let directory = workbench_directory.as_path();
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
        let record = match persisted
            .into_record_with_context(Some("test-only persisted workbench context"))
        {
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
        if records.contains_key(&record.request.run_id()) {
            quarantine_record(&path, "duplicate product-run identity", None);
            continue;
        }
        records.insert(record.request.run_id(), record);
    }
    Ok(records)
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

#[cfg(test)]
#[path = "persistence/tests.rs"]
mod tests;
