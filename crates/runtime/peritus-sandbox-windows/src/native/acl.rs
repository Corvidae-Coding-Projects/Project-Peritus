//! Exact DACL backup ownership on retained filesystem object handles.

mod anchor;
mod descriptor;
mod inheritance;
mod reservation;
mod stable_path;
pub(crate) use anchor::create;
#[cfg(test)]
mod tests;
pub(crate) use reservation::VolumeReservations;
pub(crate) use stable_path::Stabilizers;

#[cfg(test)]
#[path = "../../tests/native_enforcement/acl_fixture.rs"]
pub(crate) mod test_fixture;

use std::{
    fs::{File, OpenOptions},
    os::windows::{fs::OpenOptionsExt as _, io::AsRawHandle as _},
    path::{Path, PathBuf},
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_DISPOSITION_INFO, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO,
    FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_STANDARD_INFO,
    FileDispositionInfo, FileIdInfo, FileStandardInfo, GetFileInformationByHandle,
    GetFileInformationByHandleEx, GetFinalPathNameByHandleW, READ_CONTROL,
    SetFileInformationByHandle, WRITE_DAC,
};

use crate::{WindowsError, WindowsErrorKind, WindowsOperation, WindowsRecovery};
use descriptor::Dacl;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ObjectId {
    volume: u64,
    index: [u8; 16],
}

impl ObjectId {
    fn read(file: &File) -> Result<Self, WindowsError> {
        let mut info = FILE_ID_INFO::default();
        // SAFETY: the exact file handle is live and the output matches FileIdInfo's size/alignment.
        if unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle(),
                FileIdInfo,
                (&raw mut info).cast(),
                u32::try_from(size_of::<FILE_ID_INFO>()).expect("fixed Windows structure"),
            )
        } == 0
        {
            return Err(error("full ACL object identity cannot be queried"));
        }
        Ok(Self { volume: info.VolumeSerialNumber, index: info.FileId.Identifier })
    }
}

pub(crate) struct AclObject {
    file: File,
    original: Dacl,
    identity: ObjectId,
    directory: bool,
    original_path: PathBuf,
    #[cfg(test)]
    fail_verification_once: std::cell::Cell<bool>,
    #[cfg(test)]
    fail_inheritance_once: std::cell::Cell<bool>,
}

impl AclObject {
    pub(crate) fn capture(path: &Path) -> Result<Self, WindowsError> {
        Self::capture_reserved(path, None)
    }

    pub(crate) fn capture_reserved(
        path: &Path,
        reserved: Option<&VolumeReservations>,
    ) -> Result<Self, WindowsError> {
        let mut stable = Stabilizers::new();
        let pinned = stable.acquire(path)?;
        let access = READ_CONTROL | WRITE_DAC | FILE_READ_ATTRIBUTES;
        let file = OpenOptions::new()
            .access_mode(access)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|_| error("ACL object cannot be retained before mutation"))?;
        let object = Self::from_retained(file, path, reserved)?;
        if object.identity != pinned {
            return Err(error("ACL object identity changed during capture"));
        }
        Ok(object)
    }

    pub(crate) fn from_retained(
        file: File,
        path: &Path,
        reserved: Option<&VolumeReservations>,
    ) -> Result<Self, WindowsError> {
        let mut information = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: the owned file handle and initialized output structure remain live for the call.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &raw mut information) } == 0
            || information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err(error("ACL object identity is unavailable or is a reparse point"));
        }
        let identity = ObjectId::read(&file)?;
        if let Some(reserved) = reserved {
            reserved.verify(identity)?;
        }
        let original = Dacl::read(&file)?;
        Ok(Self {
            file,
            original,
            identity,
            directory: information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0,
            original_path: path.to_path_buf(),
            #[cfg(test)]
            fail_verification_once: std::cell::Cell::new(false),
            #[cfg(test)]
            fail_inheritance_once: std::cell::Cell::new(false),
        })
    }

    pub(crate) const fn identity(&self) -> ObjectId {
        self.identity
    }
    pub(crate) const fn is_directory(&self) -> bool {
        self.directory
    }

    pub(crate) fn delete_pending(&self) -> Result<bool, WindowsError> {
        let mut info = FILE_STANDARD_INFO::default();
        // SAFETY: the retained handle is live and output matches FileStandardInfo's layout.
        if unsafe {
            GetFileInformationByHandleEx(
                self.file.as_raw_handle(),
                FileStandardInfo,
                (&raw mut info).cast(),
                u32::try_from(size_of::<FILE_STANDARD_INFO>()).expect("fixed Windows structure"),
            )
        } == 0
        {
            return Err(error("retained ACL object deletion state cannot be queried"));
        }
        Ok(info.DeletePending)
    }

    pub(crate) fn stabilize(&self) -> Result<Stabilizers, WindowsError> {
        let mut stable = Stabilizers::new();
        self.stabilize_into(&mut stable)?;
        Ok(stable)
    }

    pub(crate) fn stabilize_into(&self, stable: &mut Stabilizers) -> Result<(), WindowsError> {
        if stable.acquire(&self.original_path)? != self.identity {
            return Err(error("planned ACL target moved or was replaced before mutation"));
        }
        Ok(())
    }

    pub(crate) fn is_original_path(&self, path: &Path) -> Result<bool, WindowsError> {
        let original = crate::WindowsPath::from_os_str(self.original_path.as_os_str())?;
        let candidate = crate::WindowsPath::from_os_str(path.as_os_str())?;
        Ok(original.same_native_path(&candidate))
    }

    pub(crate) fn stabilize_created_into(
        &self,
        stable: &mut Stabilizers,
    ) -> Result<(), WindowsError> {
        let parent =
            self.original_path.parent().ok_or_else(|| error("owned anchor has no parent"))?;
        stable.acquire(parent)?;
        // The atomic-create handle already denies DELETE sharing for this owned anchor.
        // This metadata open must share DELETE to coexist with its retained DELETE authority.
        let file = OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&self.original_path)
            .map_err(|_| error("owned ACL anchor path cannot be verified"))?;
        if ObjectId::read(&file)? != self.identity {
            return Err(error("owned ACL anchor no longer occupies its authorized path"));
        }
        Ok(())
    }

    pub(crate) const fn same_volume(&self, other: &Self) -> bool {
        self.identity.volume == other.identity.volume
    }

    pub(crate) fn path(&self) -> Result<PathBuf, WindowsError> {
        use std::{ffi::OsString, os::windows::ffi::OsStringExt as _};
        let mut buffer = vec![0_u16; 32_768];
        // SAFETY: the retained file is live and the writable buffer has the supplied length.
        let count = unsafe {
            GetFinalPathNameByHandleW(self.file.as_raw_handle(), buffer.as_mut_ptr(), 32_768, 0)
        };
        if count == 0 || count >= 32_768 {
            return Err(error("retained ACL object path cannot be resolved"));
        }
        let path = PathBuf::from(OsString::from_wide(&buffer[..count as usize]));
        crate::WindowsPath::from_canonicalized(&path).map(|path| path.to_path_buf())
    }

    pub(crate) fn restore_inheritance(&self) -> Result<(), WindowsError> {
        #[cfg(test)]
        if self.fail_inheritance_once.replace(false) {
            return Err(error("injected inheritance restore failure"));
        }
        self.original.restore_inheritance(&self.file)
    }

    pub(crate) fn stabilize_current(&self) -> Result<(PathBuf, Stabilizers), WindowsError> {
        let path = self.path()?;
        let mut stable = Stabilizers::new();
        if stable.acquire(&path)? != self.identity {
            return Err(error("cleanup subtree identity changed"));
        }
        Ok((path, stable))
    }

    pub(crate) fn current_clone(&self) -> Result<Self, WindowsError> {
        let file = self
            .file
            .try_clone()
            .map_err(|_| error("cleanup identity handle cannot be retained"))?;
        Self::from_retained(file, &self.path()?, None)
    }

    pub(crate) fn verify_new_child(
        &self,
        parent: &Self,
        principal: &str,
        temporary: &[(bool, u32)],
    ) -> Result<(), WindowsError> {
        inheritance::verify_new_child(self, parent, principal, temporary)
    }

    pub(crate) fn restore_exact(&self) -> Result<(), WindowsError> {
        self.original.restore_exact(&self.file)?;
        #[cfg(test)]
        if self.fail_verification_once.replace(false) {
            return Err(error("injected exact DACL verification failure"));
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn fail_next_verification(&self) {
        self.fail_verification_once.set(true);
    }
    #[cfg(test)]
    pub(crate) fn fail_next_inheritance(&self) {
        self.fail_inheritance_once.set(true);
    }
}

pub(crate) fn remove_created_directory(file: &File) -> Result<(), WindowsError> {
    let information = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: this is the exact retained directory handle opened with DELETE access;
    // the initialized input lives for the call. Windows rejects nonempty directories.
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            (&raw const information).cast(),
            u32::try_from(size_of::<FILE_DISPOSITION_INFO>()).expect("fixed Windows structure"),
        )
    } == 0
    {
        return Err(error("owned ACL anchor cannot be removed by identity"));
    }
    Ok(())
}

pub(super) fn error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::Acl,
        WindowsOperation::RestoreAcl,
        WindowsRecovery::RetryCleanup,
        detail,
    )
}
