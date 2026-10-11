//! Transient path stabilization for the remaining path-based icacls apply operation.

use super::{ObjectId, error};
use crate::WindowsError;
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    os::windows::{fs::OpenOptionsExt as _, io::AsRawHandle as _},
    path::{Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
    GetFileInformationByHandle,
};

pub(crate) struct Stabilizers {
    paths: BTreeMap<PathBuf, ObjectId>,
    objects: BTreeMap<ObjectId, File>,
}

impl Stabilizers {
    pub(crate) const fn new() -> Self {
        Self { paths: BTreeMap::new(), objects: BTreeMap::new() }
    }

    pub(super) fn acquire(&mut self, path: &Path) -> Result<ObjectId, WindowsError> {
        let mut components = path.ancestors().collect::<Vec<_>>();
        components.reverse();
        let mut final_identity = None;
        for component in components {
            if let Some(identity) = self.paths.get(component) {
                final_identity = Some(*identity);
                continue;
            }
            let file = OpenOptions::new()
                .access_mode(FILE_READ_ATTRIBUTES)
                // Share-delete snapshot handles remain compatible with workspace edits later;
                // these transient no-delete handles pin the complete initial apply namespace.
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(component)
                .map_err(|_| error("ACL path cannot be stabilized before mutation"))?;
            let mut information = BY_HANDLE_FILE_INFORMATION::default();
            // SAFETY: the opened component and initialized output storage remain live.
            if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &raw mut information) }
                == 0
                || information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            {
                return Err(error("ACL path component is ambiguous or a reparse point"));
            }
            let identity = ObjectId::read(&file)?;
            self.paths.insert(component.to_path_buf(), identity);
            self.objects.entry(identity).or_insert(file);
            final_identity = Some(identity);
        }
        final_identity.ok_or_else(|| error("ACL target stabilization is empty"))
    }
}
