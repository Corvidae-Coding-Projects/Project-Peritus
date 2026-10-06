//! Service-owned deterministic crash injection for recovery tests.

use super::{Error, ProductRunService, WorkbenchCommand};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::redundant_pub_crate, reason = "crate-level tests inject exact crash boundaries")]
pub(crate) enum RewindFaultPoint {
    AfterPrepare,
    AfterFolderPatch,
    InsideFolderPatch,
}

impl ProductRunService {
    pub(in crate::product_run) fn obstruct_folder_patch(
        &self,
        command: &WorkbenchCommand,
        namespace: &std::path::Path,
        patch: &peritus_patch::PatchSet,
    ) -> Result<(), Error> {
        if self.take_rewind_fault(command, RewindFaultPoint::InsideFolderPatch)? {
            // Real C1 consumes its action and hits this incomplete transaction in its patch
            // adapter, returning Reconcile. No synthetic WorkspaceError replaces that execution.
            std::fs::create_dir(namespace.join(format!("txn-{}", patch.identity().to_hex())))?;
        }
        Ok(())
    }

    pub(in crate::product_run) fn inject_rewind_fault(
        &self,
        operation: [u8; 16],
        point: RewindFaultPoint,
    ) {
        self.inner.rewind_faults.lock().expect("rewind fault lock").push((operation, point));
    }

    pub(in crate::product_run) fn check_rewind_fault(
        &self,
        command: &WorkbenchCommand,
        point: RewindFaultPoint,
    ) -> Result<(), Error> {
        if self.take_rewind_fault(command, point)? {
            Err(Error::Corrupt("injected rewind crash boundary"))
        } else {
            Ok(())
        }
    }

    fn take_rewind_fault(
        &self,
        command: &WorkbenchCommand,
        point: RewindFaultPoint,
    ) -> Result<bool, Error> {
        let mut faults =
            self.inner.rewind_faults.lock().map_err(|_| Error::Corrupt("rewind fault lock"))?;
        if let Some(index) = faults
            .iter()
            .position(|candidate| candidate == &(command.operation().into_bytes(), point))
        {
            faults.remove(index);
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

mod tests;
