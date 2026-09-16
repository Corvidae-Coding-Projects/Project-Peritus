//! Durable product-run snapshots and restart recovery.

use std::{
    collections::BTreeMap,
    fs,
    path::Path,
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
    SharedConversation, filesystem, invalid,
};
use crate::{DaemonError, DaemonErrorCode, DaemonRecovery};

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
pub fn inject_persistence_fault(run_id: RunId, point: PersistenceFaultPoint) {
    PERSISTENCE_FAULTS.lock().expect("persistence fault lock").push((run_id.into_bytes(), point));
}

#[cfg(test)]
fn check_persistence_fault(
    run_id: RunId,
    point: PersistenceFaultPoint,
) -> Result<(), ProductRunServiceError> {
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

fn write_record(directory: &Path, record: &RunRecord) -> Result<(), ProductRunServiceError> {
    use std::io::Write as _;
    let workbench_directory;
    let directory =
        if record.interaction.as_ref().is_some_and(|options| options.workbench.is_some()) {
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
    let path = directory.join(format!("{}.json", persisted.run_id));
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
        let entry = entry.map_err(filesystem)?;
        if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let bytes = fs::read(entry.path()).map_err(filesystem)?;
        let persisted: PersistedRecord = serde_json::from_slice(&bytes).map_err(|error| {
            DaemonError::with_source(
                DaemonErrorCode::CorruptState,
                DaemonRecovery::Reconcile,
                "load product run",
                "product-run state is malformed",
                error,
            )
        })?;
        let record = persisted
            .into_record()
            .map_err(|_| invalid("product-run state contains invalid values"))?;
        if record.interaction.as_ref().is_some_and(|options| options.workbench.is_some()) {
            return Err(invalid(
                "workbench execution state cannot be loaded from the legacy generation",
            ));
        }
        records.insert(record.request.run_id(), record);
    }
    Ok(records)
}

mod record;
#[cfg(test)]
use record::hex;

#[cfg(test)]
#[path = "persistence/tests.rs"]
mod tests;
