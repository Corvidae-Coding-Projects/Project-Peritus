//! Operation-bound deterministic crash injection for recovery tests.

use super::{Error, WorkbenchCommand};

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::redundant_pub_crate, reason = "crate-level tests inject exact crash boundaries")]
pub(crate) enum RewindFaultPoint {
    AfterPrepare,
    AfterFolderPatch,
    InsideFolderPatch,
}

#[allow(clippy::redundant_pub_crate, reason = "sibling C1 caller injects exact test boundary")]
pub(crate) fn obstruct_folder_patch(
    command: &WorkbenchCommand,
    namespace: &std::path::Path,
    patch: &peritus_patch::PatchSet,
) -> Result<(), Error> {
    let mut faults = REWIND_FAULTS.lock().map_err(|_| Error::Corrupt("rewind fault lock"))?;
    if let Some(index) = faults.iter().position(|candidate| {
        candidate == &(command.operation().into_bytes(), RewindFaultPoint::InsideFolderPatch)
    }) {
        faults.remove(index);
        // Real C1 will consume its action and hit this incomplete transaction in its patch
        // adapter, returning Reconcile. No synthetic WorkspaceError replaces that execution.
        std::fs::create_dir(namespace.join(format!("txn-{}", patch.identity().to_hex())))?;
    }
    Ok(())
}

#[cfg(test)]
static REWIND_FAULTS: std::sync::Mutex<Vec<([u8; 16], RewindFaultPoint)>> =
    std::sync::Mutex::new(Vec::new());

#[cfg(test)]
#[allow(clippy::redundant_pub_crate, reason = "crate-level tests inject exact crash boundaries")]
pub(crate) fn inject_rewind_fault(operation: [u8; 16], point: RewindFaultPoint) {
    REWIND_FAULTS.lock().expect("rewind fault lock").push((operation, point));
}

#[cfg(test)]
pub(super) fn check_rewind_fault(
    command: &WorkbenchCommand,
    point: RewindFaultPoint,
) -> Result<(), Error> {
    let mut faults = REWIND_FAULTS.lock().map_err(|_| Error::Corrupt("rewind fault lock"))?;
    if let Some(index) =
        faults.iter().position(|candidate| candidate == &(command.operation().into_bytes(), point))
    {
        faults.remove(index);
        Err(Error::Corrupt("injected rewind crash boundary"))
    } else {
        Ok(())
    }
}

#[cfg(not(test))]
pub(super) const fn check_rewind_fault(
    _command: &WorkbenchCommand,
    _point: (),
) -> Result<(), Error> {
    Ok(())
}
