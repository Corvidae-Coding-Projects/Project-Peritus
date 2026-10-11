//! Cross-process, movable ACL exclusion for each complete filesystem volume.
//!
//! Existence and last-close semantics: <https://learn.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-createmutexw>

use super::{ObjectId, error};
use crate::{
    AclPlan, ResolvedWindowsPath, WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::OpenOptions,
    os::windows::{
        fs::{MetadataExt as _, OpenOptionsExt as _},
        io::{FromRawHandle as _, IntoRawHandle as _, OwnedHandle},
    },
};
use windows_sys::Win32::{
    Foundation::{ERROR_ALREADY_EXISTS, GetLastError, SetLastError},
    Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    },
    System::Threading::CreateMutexW,
};

pub(crate) struct VolumeReservations(BTreeMap<u64, OwnedHandle>);

impl VolumeReservations {
    pub(crate) const fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub(crate) fn acquire(plan: &AclPlan) -> Result<Self, WindowsError> {
        let mut volumes = BTreeSet::new();
        // These probes request FILE_READ_ATTRIBUTES only. No pristine DACL is read until
        // every sorted volume is reserved; missing anchors use their existing parent.
        for entry in plan.entries() {
            let (resolved, _) =
                ResolvedWindowsPath::resolve_existing_or_parent(entry.path().clone())?;
            // Share DELETE here so an existing transaction's atomically owned deny anchor
            // can still be probed. Namespace pinning belongs to the post-reservation phase.
            let file = OpenOptions::new()
                .access_mode(FILE_READ_ATTRIBUTES)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(resolved.evidence().resolved().to_path_buf())
                .map_err(|_| probe_error("ACL reservation volume cannot be probed"))?;
            let metadata = file
                .metadata()
                .map_err(|_| probe_error("ACL volume probe identity is unavailable"))?;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(probe_error("ACL volume probe became a reparse point"));
            }
            let identity = ObjectId::read(&file)
                .map_err(|_| probe_error("full ACL volume identity cannot be probed"))?;
            volumes.insert(identity.volume);
        }
        let mut reservations = Self::new();
        for volume in volumes {
            let name = format!("Global\\Peritus.AclVolume.v1.{volume:016X}")
                .encode_utf16()
                .chain([0])
                .collect::<Vec<_>>();
            // SAFETY: a terminated name is live; null attributes make the handle noninheritable.
            // Initial ownership is FALSE: existence, never thread-affine mutex ownership, is
            // the reservation. The last handle close releases it on any thread.
            let (raw, status) = unsafe {
                SetLastError(0);
                let raw = CreateMutexW(core::ptr::null(), 0, name.as_ptr());
                (raw, GetLastError())
            };
            let handle = if raw.is_null() {
                None
            } else {
                // SAFETY: CreateMutexW returned a new owned handle; this transfers it exactly once.
                Some(unsafe { OwnedHandle::from_raw_handle(raw) })
            };
            if status == ERROR_ALREADY_EXISTS {
                return Err(WindowsError::new(
                    WindowsErrorKind::Acl,
                    WindowsOperation::InstallAcl,
                    WindowsRecovery::Replan,
                    "another ACL transaction reserves this volume",
                ));
            }
            if status != 0 || handle.is_none() {
                return Err(WindowsError::new(
                    WindowsErrorKind::Acl,
                    WindowsOperation::InstallAcl,
                    WindowsRecovery::ConfigureHost,
                    "cross-process ACL volume reservation is unavailable",
                ));
            }
            if let Some(handle) = handle {
                reservations.0.insert(volume, handle);
            }
        }
        Ok(reservations)
    }

    pub(super) fn verify(&self, identity: ObjectId) -> Result<(), WindowsError> {
        if self.0.contains_key(&identity.volume) {
            Ok(())
        } else {
            Err(error("ACL object escaped the reserved volumes before snapshot"))
        }
    }

    /// Losing the final cleanup owner must not admit another transaction on a dirty baseline.
    /// These deliberate process-lifetime handles close only at OS teardown; this is not crash
    /// recovery and does not preserve the discarded transaction's descriptor preimages.
    pub(crate) fn quarantine_process_lifetime(&mut self) {
        for (_, handle) in core::mem::take(&mut self.0) {
            let _quarantined = handle.into_raw_handle();
        }
    }

    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }
}

fn probe_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::Acl,
        WindowsOperation::InstallAcl,
        WindowsRecovery::Replan,
        detail,
    )
}
