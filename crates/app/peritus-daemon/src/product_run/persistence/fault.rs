//! Deterministic persistence fault scheduling for recovery qualification.

use std::path::{Path, PathBuf};

use peritus_types::RunId;

use super::ProductRunServiceError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistenceFaultPoint {
    BeforeWrite,
    BeforeFileSync,
    BeforeRename,
    AfterRename,
    BeforeDirectorySync,
}

static PERSISTENCE_FAULTS: std::sync::Mutex<Vec<(PathBuf, [u8; 16], PersistenceFaultPoint)>> =
    std::sync::Mutex::new(Vec::new());

static PERSISTENT_PERSISTENCE_FAULTS: std::sync::Mutex<
    Vec<(PathBuf, [u8; 16], PersistenceFaultPoint)>,
> = std::sync::Mutex::new(Vec::new());

pub fn inject_persistence_fault(directory: &Path, run_id: RunId, point: PersistenceFaultPoint) {
    PERSISTENCE_FAULTS.lock().expect("persistence fault lock").push((
        directory.to_path_buf(),
        run_id.into_bytes(),
        point,
    ));
}

pub fn inject_persistent_persistence_fault(
    directory: &Path,
    run_id: RunId,
    point: PersistenceFaultPoint,
) {
    PERSISTENT_PERSISTENCE_FAULTS.lock().expect("persistent persistence fault lock").push((
        directory.to_path_buf(),
        run_id.into_bytes(),
        point,
    ));
}

pub fn clear_persistent_persistence_fault(
    directory: &Path,
    run_id: RunId,
    point: PersistenceFaultPoint,
) {
    PERSISTENT_PERSISTENCE_FAULTS
        .lock()
        .expect("persistent persistence fault lock")
        .retain(|candidate| candidate != &(directory.to_path_buf(), run_id.into_bytes(), point));
}

pub(super) fn check_persistence_fault(
    directory: &Path,
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
        .contains(&(directory.to_path_buf(), run_id.into_bytes(), point))
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
    if let Some(index) = faults
        .iter()
        .position(|candidate| candidate == &(directory.to_path_buf(), run_id.into_bytes(), point))
    {
        faults.remove(index);
        return Err(ProductRunServiceError::persistence(
            persistence_fault_operation(point),
            "injected persistence failure",
        ));
    }
    Ok(())
}

const fn persistence_fault_operation(point: PersistenceFaultPoint) -> &'static str {
    match point {
        PersistenceFaultPoint::BeforeWrite => "write the product-run temporary record",
        PersistenceFaultPoint::BeforeFileSync => "sync the product-run temporary record",
        PersistenceFaultPoint::BeforeRename => "replace the durable product-run record",
        PersistenceFaultPoint::AfterRename => "complete durable product-run replacement",
        PersistenceFaultPoint::BeforeDirectorySync => "sync the product-run directory",
    }
}
